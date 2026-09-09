//! Window placement uses physical coordinates, including monitor work areas and DPI.
use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Rect {
    fn overlaps(self, other: Self, gap: i32) -> bool {
        self.x < other.x + other.w + gap
            && other.x < self.x + self.w + gap
            && self.y < other.y + other.h + gap
            && other.y < self.y + self.h + gap
    }
    fn contains(self, other: Self) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.w <= self.x + self.w
            && other.y + other.h <= self.y + self.h
    }
    fn clamp(self, area: Self) -> Self {
        Self {
            x: self.x.clamp(area.x, area.x + (area.w - self.w).max(0)),
            y: self.y.clamp(area.y, area.y + (area.h - self.h).max(0)),
            ..self
        }
    }
}

fn position(
    size: Rect,
    area: Rect,
    occupied: &[Rect],
    gap: i32,
    near: bool,
    title: i32,
) -> (Rect, bool) {
    let mut xs = vec![area.x, size.clamp(area).x];
    let mut ys = vec![area.y, size.clamp(area).y];
    for r in occupied {
        xs.extend([r.x + r.w + gap, r.x - size.w - gap]);
        ys.extend([r.y + r.h + gap, r.y - size.h - gap]);
    }
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    let mut candidates = Vec::new();
    for y in ys {
        for &x in &xs {
            let r = Rect { x, y, ..size };
            if area.contains(r) && !occupied.iter().any(|o| r.overlaps(*o, gap)) {
                candidates.push(r);
            }
        }
    }
    candidates.sort_by_key(|r| {
        (
            if near {
                i64::from(r.x - size.x).pow(2) + i64::from(r.y - size.y).pow(2)
            } else {
                0
            },
            r.y,
            r.x,
        )
    });
    if let Some(r) = candidates.first() {
        return (*r, false);
    }
    // Overflow: stagger within the work area, preferring unoccupied title strips.
    // No algorithm can expose every title when the screen itself is too small.
    let mut best = (usize::MAX, size.clamp(area));
    let max_x = area.x + (area.w - size.w).max(0);
    let max_y = area.y + (area.h - size.h).max(0);
    for y in (area.y..=max_y).step_by((title + gap).max(1) as usize) {
        for x in (area.x..=max_x).step_by((64 + gap).max(1) as usize) {
            let r = Rect { x, y, ..size };
            let strip = Rect {
                h: title.min(size.h),
                ..r
            };
            let collisions = occupied
                .iter()
                .filter(|o| {
                    strip.overlaps(
                        Rect {
                            h: title.min(o.h),
                            ..**o
                        },
                        gap,
                    )
                })
                .count();
            if collisions < best.0 {
                best = (collisions, r);
            }
            if collisions == 0 {
                return (r, true);
            }
        }
    }
    (best.1, true)
}

#[derive(Default)]
pub struct LayoutState {
    undo: Mutex<Vec<(String, Rect)>>,
    operation: Mutex<()>,
}

struct Item {
    window: WebviewWindow,
    rect: Rect,
    area: Rect,
    gap: i32,
    title: i32,
}

fn item(window: WebviewWindow) -> anyhow::Result<Item> {
    let monitor = window
        .current_monitor()?
        .ok_or_else(|| anyhow::anyhow!("无法获取显示器；当前桌面可能不支持定位窗口"))?;
    let area = monitor.work_area();
    let p = window.outer_position()?;
    let s = window.outer_size()?;
    Ok(Item {
        rect: Rect {
            x: p.x,
            y: p.y,
            w: s.width as i32,
            h: s.height as i32,
        },
        area: Rect {
            x: area.position.x,
            y: area.position.y,
            w: area.size.width as i32,
            h: area.size.height as i32,
        },
        gap: (10.0 * monitor.scale_factor()).round() as i32,
        title: (40.0 * window.scale_factor()?).round() as i32,
        window,
    })
}

fn move_to(window: &WebviewWindow, rect: Rect) -> anyhow::Result<()> {
    window.set_position(PhysicalPosition::new(rect.x, rect.y))?;
    // Persist the intended coordinates, not a potentially stale asynchronous WM reply.
    super::windows::remember_position(window.app_handle(), window.label(), rect.x, rect.y)
}

#[tauri::command]
pub async fn arrange_notes(app: AppHandle, undo: bool) -> Result<String, String> {
    arrange(&app, undo).map_err(|error| error.to_string())
}

pub fn arrange(app: &AppHandle, undo: bool) -> anyhow::Result<String> {
    anyhow::ensure!(!crate::updates::preparing(), "正在准备更新，请稍候");
    let state = app.state::<LayoutState>();
    let _guard = state
        .operation
        .try_lock()
        .map_err(|_| anyhow::anyhow!("正在整理，请稍候"))?;
    if undo {
        let saved = state.undo.lock().unwrap().clone();
        anyhow::ensure!(!saved.is_empty(), "没有可以撤销的整理");
        let monitors = app.available_monitors()?;
        for (label, rect) in &saved {
            if let Some(window) = app.get_webview_window(label) {
                let current = item(window)?;
                // If a monitor was unplugged, keep the restored title accessible.
                let reachable = monitors.iter().any(|m| {
                    let a = m.work_area();
                    Rect {
                        x: a.position.x,
                        y: a.position.y,
                        w: a.size.width as i32,
                        h: a.size.height as i32,
                    }
                    .contains(Rect {
                        w: 100,
                        h: current.title,
                        ..*rect
                    })
                });
                move_to(
                    &current.window,
                    if reachable {
                        *rect
                    } else {
                        rect.clamp(current.area)
                    },
                )?;
            }
        }
        state.undo.lock().unwrap().clear();
        return Ok("已恢复整理前的位置".into());
    }
    let mut items = Vec::new();
    for (label, window) in app.webview_windows() {
        if label.starts_with("note-") && window.is_visible()? && !window.is_minimized()? {
            items.push(item(window)?);
        }
    }
    anyhow::ensure!(!items.is_empty(), "没有可整理的可见便签");
    items.sort_by(|a, b| {
        b.rect
            .h
            .cmp(&a.rect.h)
            .then_with(|| a.window.label().cmp(b.window.label()))
    });
    let saved: Vec<_> = items
        .iter()
        .map(|i| (i.window.label().to_owned(), i.rect))
        .collect();
    let mut placed: Vec<(Rect, Rect)> = Vec::new();
    let mut overflow = false;
    let mut targets = Vec::new();
    for i in &items {
        let occupied: Vec<_> = placed
            .iter()
            .filter(|(a, _)| *a == i.area)
            .map(|(_, r)| *r)
            .collect();
        let (r, crowded) = position(
            Rect {
                x: i.area.x,
                y: i.area.y,
                ..i.rect
            },
            i.area,
            &occupied,
            i.gap,
            false,
            i.title,
        );
        overflow |= crowded;
        placed.push((i.area, r));
        targets.push(r);
    }
    // Keep the snapshot even if the window manager rejects a later move.
    *state.undo.lock().unwrap() = saved;
    for (i, r) in items.iter().zip(targets) {
        move_to(&i.window, r)?;
    }
    Ok(format!(
        "已整理 {} 张便签{}",
        items.len(),
        if overflow {
            "；空间不足的便签已错位排列"
        } else {
            ""
        }
    ))
}

pub fn avoid_collapsed(window: &WebviewWindow, width: u32) -> anyhow::Result<()> {
    let app = window.app_handle();
    let state = app.state::<LayoutState>();
    let _guard = state
        .operation
        .try_lock()
        .map_err(|_| anyhow::anyhow!("正在整理，请稍候"))?;
    let mut current = item(window.clone())?;
    current.rect.w = (width as f64 * window.scale_factor()?).round() as i32;
    current.rect.h = current.title;
    let mut occupied = Vec::new();
    for (label, other) in app.webview_windows() {
        if label.starts_with("note-")
            && label != window.label()
            && other.is_visible()?
            && !other.is_minimized()?
        {
            let other = item(other)?;
            if other.area == current.area {
                occupied.push(other.rect);
            }
        }
    }
    if current.area.contains(current.rect) && !occupied.iter().any(|o| current.rect.overlaps(*o, 0))
    {
        return Ok(());
    }
    let (rect, _) = position(
        current.rect,
        current.area,
        &occupied,
        current.gap,
        true,
        current.title,
    );
    move_to(window, rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_sizes_fit_without_overlap() {
        let area = Rect {
            x: -1200,
            y: 30,
            w: 1200,
            h: 800,
        };
        let mut placed = Vec::new();
        for (w, h) in [(420, 460), (310, 330), (310, 40), (400, 40)] {
            let (r, overflow) = position(
                Rect {
                    x: area.x,
                    y: area.y,
                    w,
                    h,
                },
                area,
                &placed,
                10,
                false,
                40,
            );
            assert!(!overflow);
            assert!(area.contains(r));
            assert!(placed.iter().all(|o| !r.overlaps(*o, 10)));
            placed.push(r);
        }
    }
    #[test]
    fn collapsed_moves_nearby_without_resizing() {
        let area = Rect {
            x: 0,
            y: 0,
            w: 1000,
            h: 700,
        };
        let note = Rect {
            x: 200,
            y: 200,
            w: 310,
            h: 40,
        };
        let (r, _) = position(note, area, &[note], 10, true, 40);
        assert_eq!((r.w, r.h), (310, 40));
        assert!(!r.overlaps(note, 10));
        assert_eq!(r.x, 200);
        assert_eq!((r.y - 200).abs(), 50);
    }
    #[test]
    fn overflow_staggers_and_oversize_remains_reachable() {
        let area = Rect {
            x: 0,
            y: 0,
            w: 500,
            h: 500,
        };
        let note = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        };
        let (r, overflow) = position(note, area, &[note], 10, false, 40);
        assert!(overflow);
        assert_ne!((r.x, r.y), (0, 0));
        assert!(area.contains(r));
        let huge = Rect {
            w: 900,
            h: 900,
            ..note
        };
        assert_eq!(position(huge, area, &[], 10, false, 40).0, huge);
    }
}

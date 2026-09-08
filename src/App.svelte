<script lang="ts">
  import Library from './Library.svelte';
  import Editor from './Editor.svelte';
  const noteId = new URLSearchParams(location.search).get('note');
  if (noteId) document.body.classList.add('note-window');
</script>

{#if noteId}<Editor {noteId} standalone />{:else}<Library />{/if}

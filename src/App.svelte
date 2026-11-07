<script lang="ts">
  import Library from "./lib/Library.svelte";
  import Workspace from "./lib/Workspace.svelte";
  import Overlays from "./lib/Overlays.svelte";

  let project = $state<string | null>(null);
</script>

{#if project}
  {#key project}
    <Workspace path={project} onBack={() => (project = null)} />
  {/key}
{:else}
  <Library onOpen={(p) => (project = p)} />
{/if}
<Overlays />

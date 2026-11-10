<script lang="ts">
  import { filledIcons, icons, type IconName } from "../icons";

  let { name, size = 20, strokeWidth = 2, fill = false }: { name: IconName; size?: number; strokeWidth?: number; fill?: boolean } = $props();
  const filled = $derived(filledIcons.has(name));
</script>

<script lang="ts" module>
  // Long lists mount many rows at once while scrolling; parsing each icon's markup every time
  // is the slow part, so each icon is parsed once and cloned after that.
  const parsed = new Map<IconName, DocumentFragment>();

  function paint(node: SVGSVGElement, name: IconName) {
    const draw = (n: IconName) => {
      let frag = parsed.get(n);
      if (!frag) {
        const tpl = document.createElementNS("http://www.w3.org/2000/svg", "svg");
        tpl.innerHTML = icons[n];
        frag = document.createDocumentFragment();
        frag.append(...tpl.childNodes);
        parsed.set(n, frag);
      }
      node.replaceChildren(frag.cloneNode(true));
    };
    draw(name);
    return { update: draw };
  }
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={filled || fill ? "currentColor" : "none"}
  stroke={filled ? "none" : "currentColor"}
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  use:paint={name}></svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>

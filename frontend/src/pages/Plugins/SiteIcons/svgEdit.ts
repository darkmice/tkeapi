export interface SvgMeta {
  width: number;
  height: number;
  background: string | null;
}

export interface SvgEdits {
  width: number;
  height: number;
  background: string | null;
  iconColor: string | null;
}

const BG_ATTR = 'data-tb-bg';
const PLATE_ATTR = 'data-tb-plate';
const SHAPES = new Set(['path', 'circle', 'ellipse', 'polygon', 'polyline', 'rect', 'line', 'text']);

function parseSvg(source: string): SVGSVGElement | null {
  const doc = new DOMParser().parseFromString(source, 'image/svg+xml');
  const root = doc.documentElement;
  if (!root || root.nodeName.toLowerCase() !== 'svg') return null;
  if (doc.querySelector('parsererror')) return null;
  return root as unknown as SVGSVGElement;
}

function viewBoxSize(el: SVGSVGElement): { width: number; height: number } {
  const parts = (el.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number);
  if (parts.length === 4 && parts[2] > 0 && parts[3] > 0) {
    return { width: parts[2], height: parts[3] };
  }
  return { width: 24, height: 24 };
}

function readLength(raw: string | null, fallback: number): number {
  if (!raw) return fallback;
  const matched = raw.trim().match(/^(\d+(?:\.\d+)?)(px)?$/i);
  if (!matched) return fallback;
  const n = Number(matched[1]);
  return n > 0 ? n : fallback;
}

function colorToken(value: string): string {
  return value.replace(/!important/gi, '').trim().replace(/^['"]|['"]$/g, '');
}

function isPaint(value: string): boolean {
  const s = colorToken(value).toLowerCase();
  if (!s || s === 'none' || s === 'transparent' || s.startsWith('url(') || s.startsWith('var(')) return false;
  return true;
}

function recolorCss(css: string, color: string): string {
  return css.replace(/(fill|stroke|stop-color)\s*:\s*([^;}{]+)/gi, (all, prop: string, value: string) => {
    if (!isPaint(value)) return all;
    return `${prop}: ${color}`;
  });
}

function paintFill(node: Element, color: string) {
  node.setAttribute('fill', color);
  const style = node.getAttribute('style');
  if (style && /fill\s*:/i.test(style)) {
    node.setAttribute('style', style.replace(/fill\s*:\s*[^;}{]+/gi, `fill: ${color}`));
  }
}

function ensureViewBox(el: SVGSVGElement, width: number, height: number) {
  const parts = (el.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number);
  if (parts.length === 4 && parts.every((n) => Number.isFinite(n)) && parts[2] > 0 && parts[3] > 0) {
    return { x: parts[0], y: parts[1], w: parts[2], h: parts[3] };
  }
  el.setAttribute('viewBox', `0 0 ${width} ${height}`);
  return { x: 0, y: 0, w: width, h: height };
}

function markBackgroundPlates(el: SVGSVGElement, box: { x: number; y: number; w: number; h: number }) {
  el.querySelectorAll(`[${PLATE_ATTR}]`).forEach((node) => node.removeAttribute(PLATE_ATTR));
  const nodes = Array.from(el.querySelectorAll('*'));
  const measured = measureShapes(el, nodes);
  nodes.forEach((node, index) => {
    if (node.getAttribute(BG_ATTR) != null) return;
    const tag = node.tagName.toLowerCase();
    if (!SHAPES.has(tag)) return;
    const fill = node.getAttribute('fill');
    const style = node.getAttribute('style') || '';
    if (fill === 'none' || (!fill && !/fill\s*:/i.test(style))) return;
    const bounds = measured[index];
    const covers = bounds
      ? bounds.w >= box.w * 0.88 && bounds.h >= box.h * 0.88
      : tag === 'rect' && readLength(node.getAttribute('width'), 0) >= box.w * 0.88 && readLength(node.getAttribute('height'), 0) >= box.h * 0.88;
    if (covers) node.setAttribute(PLATE_ATTR, '1');
  });
}

function measureShapes(el: SVGSVGElement, nodes: Element[]): Array<{ w: number; h: number } | null> {
  if (typeof document === 'undefined') return nodes.map(() => null);
  const box = viewBoxSize(el);
  const host = document.createElement('div');
  host.style.cssText = `position:fixed;left:0;top:0;width:${box.width}px;height:${box.height}px;overflow:hidden;opacity:0;pointer-events:none;z-index:-1`;
  const clone = document.importNode(el, true) as SVGSVGElement;
  clone.setAttribute('width', String(box.width));
  clone.setAttribute('height', String(box.height));
  host.appendChild(clone);
  document.body.appendChild(host);
  const clones = Array.from(clone.querySelectorAll('*'));
  const measured = nodes.map((_node, index) => {
    const target = clones[index] as SVGGraphicsElement | undefined;
    if (!target || typeof target.getBBox !== 'function') return null;
    try {
      const b = target.getBBox();
      if (b.width <= 0 || b.height <= 0) return null;
      return { w: b.width, h: b.height };
    } catch {
      return null;
    }
  });
  host.remove();
  return measured;
}

function applyBackground(el: SVGSVGElement, color: string | null, width: number, height: number) {
  const existing = el.querySelector(`[${BG_ATTR}]`);
  if (!color) {
    existing?.remove();
    return;
  }
  const box = ensureViewBox(el, width, height);
  el.querySelectorAll(`[${PLATE_ATTR}]`).forEach((node) => paintFill(node, color));
  const doc = el.ownerDocument;
  let rect = existing as SVGRectElement | null;
  if (!rect) {
    rect = doc.createElementNS('http://www.w3.org/2000/svg', 'rect');
    rect.setAttribute(BG_ATTR, '1');
    el.insertBefore(rect, el.firstChild);
  }
  rect.setAttribute('x', String(box.x));
  rect.setAttribute('y', String(box.y));
  rect.setAttribute('width', String(box.w));
  rect.setAttribute('height', String(box.h));
  rect.setAttribute('fill', color);
}

function recolorIcon(el: SVGSVGElement, color: string) {
  el.querySelectorAll('style').forEach((node) => {
    node.textContent = recolorCss(node.textContent || '', color);
  });
  [el, ...Array.from(el.querySelectorAll('*'))].forEach((node) => {
    if (node.getAttribute(BG_ATTR) != null || node.getAttribute(PLATE_ATTR) != null) return;
    for (const attr of ['fill', 'stroke', 'stop-color']) {
      const value = node.getAttribute(attr);
      if (value && isPaint(value)) node.setAttribute(attr, color);
    }
    const style = node.getAttribute('style');
    if (style) node.setAttribute('style', recolorCss(style, color));
    const tag = node.tagName.toLowerCase();
    if (!SHAPES.has(tag)) return;
    const fill = node.getAttribute('fill');
    if (fill === 'none') return;
    const hasFill = !!fill || /fill\s*:/i.test(node.getAttribute('style') || '') || node.hasAttribute('class');
    if (!hasFill) node.setAttribute('fill', color);
  });
}

export function readSvgMeta(source: string): SvgMeta {
  const el = parseSvg(source);
  if (!el) return { width: 24, height: 24, background: null };
  const vb = viewBoxSize(el);
  const bg = el.querySelector(`[${BG_ATTR}]`)?.getAttribute('fill') || null;
  return {
    width: Math.round(readLength(el.getAttribute('width'), vb.width)),
    height: Math.round(readLength(el.getAttribute('height'), vb.height)),
    background: bg,
  };
}

export function applySvgEdits(source: string, edits: SvgEdits): string {
  const el = parseSvg(source);
  if (!el) return source;
  const width = Math.round(edits.width);
  const height = Math.round(edits.height);
  if (width > 0) el.setAttribute('width', String(width));
  if (height > 0) el.setAttribute('height', String(height));
  const box = ensureViewBox(el, width > 0 ? width : 24, height > 0 ? height : 24);
  markBackgroundPlates(el, box);
  applyBackground(el, edits.background, box.w, box.h);
  if (edits.iconColor) recolorIcon(el, edits.iconColor);
  el.querySelectorAll(`[${PLATE_ATTR}]`).forEach((node) => node.removeAttribute(PLATE_ATTR));
  return new XMLSerializer().serializeToString(el);
}

export function fitSvgPreview(source: string): string {
  return source.replace(/<svg\b([^>]*)>/i, (_all, attrs: string) => {
    const cleaned = attrs
      .replace(/\swidth\s*=\s*(['"]).*?\1/i, '')
      .replace(/\sheight\s*=\s*(['"]).*?\1/i, '')
      .replace(/\sstyle\s*=\s*(['"]).*?\1/i, '');
    return `<svg${cleaned} width="56" height="56" style="display:block">`;
  });
}

/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button, Modal, Typography } from 'antd';
import { RotateRightOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';

const OUTPUT_SIDE = 256;
const DEFAULT_BOX_SIDE = 256;
const VIEW_SIDE_MIN = 320;
const VIEW_SIDE_MAX = 720;
const VIEW_PAD = 12;
const MIN_BOX_SIDE = 40;
const SOURCE_MAX_EDGE = 4096;
const MAX_ZOOM = 16;
const REFIT_MS = 260;

type Corner = 'nw' | 'ne' | 'sw' | 'se';
/** 剪裁框，视口像素 */
type Box = { x: number; y: number; side: number };
/** 图片在视口中的位置：视口坐标 = x/y + 图片像素 × scale */
type View = { scale: number; x: number; y: number };
type Frame = { view: View; box: Box };
type Drag = { mode: 'pan' | Corner; startX: number; startY: number } & Frame;

const CORNER_SIGN: Record<Corner, [number, number]> = { nw: [-1, -1], ne: [1, -1], sw: [-1, 1], se: [1, 1] };

const gridDash = (direction: 'bottom' | 'right') =>
  `repeating-linear-gradient(to ${direction}, #fff 0 4px, transparent 4px 8px)`;
const stroke = (along: 'x' | 'y', edge: React.CSSProperties): React.CSSProperties => ({
  position: 'absolute',
  pointerEvents: 'none',
  backgroundImage: gridDash(along === 'x' ? 'right' : 'bottom'),
  ...(along === 'x' ? { height: 1 } : { width: 1 }),
  ...edge,
});
const GRID_LINES: { key: string; style: React.CSSProperties }[] = ['33.333%', '66.667%'].flatMap(at => [
  { key: `v${at}`, style: stroke('y', { top: 0, bottom: 0, left: at }) },
  { key: `h${at}`, style: stroke('x', { left: 0, right: 0, top: at }) },
]);

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max);

/** 预览区边长随浏览器窗口取值，扣除弹窗标题、提示与按钮的高度 */
const measureViewSide = () =>
  Math.round(clamp(Math.min(window.innerWidth - 80, window.innerHeight - 240), VIEW_SIDE_MIN, VIEW_SIDE_MAX));

const fitScale = (viewSide: number, width: number, height: number) => Math.min(viewSide / width, viewSide / height);

const initialFrame = (viewSide: number, width: number, height: number): Frame => {
  const scale = fitScale(viewSide, width, height);
  const w = width * scale;
  const h = height * scale;
  const side = Math.min(DEFAULT_BOX_SIDE, w, h, viewSide - VIEW_PAD * 2);
  return {
    view: { scale, x: (viewSide - w) / 2, y: (viewSide - h) / 2 },
    box: { x: (viewSide - side) / 2, y: (viewSide - side) / 2, side },
  };
};

const sourceSize = (image: HTMLImageElement): [number, number] => {
  const ratio = Math.min(1, SOURCE_MAX_EDGE / Math.max(image.naturalWidth, image.naturalHeight));
  return [Math.max(1, Math.round(image.naturalWidth * ratio)), Math.max(1, Math.round(image.naturalHeight * ratio))];
};

const rotateSource = (image: HTMLImageElement, turns: number): HTMLCanvasElement => {
  const odd = turns % 2 === 1;
  const [width, height] = sourceSize(image);
  const canvas = document.createElement('canvas');
  canvas.width = odd ? height : width;
  canvas.height = odd ? width : height;
  const ctx = canvas.getContext('2d');
  if (ctx) {
    ctx.imageSmoothingQuality = 'high';
    ctx.translate(canvas.width / 2, canvas.height / 2);
    ctx.rotate((turns * Math.PI) / 2);
    ctx.drawImage(image, -width / 2, -height / 2, width, height);
  }
  return canvas;
};

interface AvatarCropModalProps {
  file: File;
  onCancel: () => void;
  onConfirm: (blob: Blob) => Promise<void> | void;
}

const AvatarCropModal: React.FC<AvatarCropModalProps> = ({ file, onCancel, onConfirm }) => {
  const { t } = useTranslation();
  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const [failed, setFailed] = useState(false);
  const [turns, setTurns] = useState(0);
  const [frame, setFrame] = useState<Frame | null>(null);
  const [dragMode, setDragMode] = useState<Drag['mode'] | null>(null);
  const [saving, setSaving] = useState(false);
  const [viewSide] = useState(measureViewSide);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const refitRef = useRef(0);
  const wheelRef = useRef<((e: WheelEvent, rect: DOMRect) => void) | null>(null);

  useEffect(() => {
    let alive = true;
    const url = URL.createObjectURL(file);
    const img = new Image();
    img.onload = () => {
      if (!alive) return;
      setFailed(false);
      setImage(img);
      setFrame(initialFrame(viewSide, ...sourceSize(img)));
    };
    img.onerror = () => {
      if (alive) setFailed(true);
    };
    img.src = url;
    return () => {
      alive = false;
      URL.revokeObjectURL(url);
    };
  }, [file, viewSide]);

  const source = useMemo(() => (image ? rotateSource(image, turns) : null), [image, turns]);
  const sourceWidth = source?.width ?? 1;
  const sourceHeight = source?.height ?? 1;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !source || !frame) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(viewSide * dpr);
    canvas.height = Math.round(viewSide * dpr);
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.imageSmoothingQuality = 'high';
    const { scale, x, y } = frame.view;
    ctx.drawImage(source, x, y, source.width * scale, source.height * scale);
  }, [source, frame, viewSide]);

  /** 图片必须始终盖住剪裁框 */
  const coverBox = (scale: number, x: number, y: number, box: Box): View => ({
    scale,
    x: clamp(x, box.x + box.side - sourceWidth * scale, box.x),
    y: clamp(y, box.y + box.side - sourceHeight * scale, box.y),
  });

  const maxScale = fitScale(viewSide, sourceWidth, sourceHeight) * MAX_ZOOM;

  const stopRefit = () => cancelAnimationFrame(refitRef.current);

  useEffect(() => {
    const ref = refitRef;
    return () => cancelAnimationFrame(ref.current);
  }, []);

  /** 剪裁框缩放松手后：框回到默认大小并居中，框内选区跟着放大（或缩小）铺满 */
  const refit = (from: Frame) => {
    const region = {
      x: (from.box.x - from.view.x) / from.view.scale,
      y: (from.box.y - from.view.y) / from.view.scale,
      side: from.box.side / from.view.scale,
    };
    const defaultSide = initialFrame(viewSide, sourceWidth, sourceHeight).box.side;
    const targetSide = Math.min(defaultSide, region.side * Math.max(maxScale, from.view.scale));
    const target: Box = { x: (viewSide - targetSide) / 2, y: (viewSide - targetSide) / 2, side: targetSide };
    const start = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / REFIT_MS);
      const e = 1 - (1 - t) ** 3;
      const box: Box = {
        x: from.box.x + (target.x - from.box.x) * e,
        y: from.box.y + (target.y - from.box.y) * e,
        side: from.box.side + (target.side - from.box.side) * e,
      };
      const scale = box.side / region.side;
      setFrame({ box, view: coverBox(scale, box.x - region.x * scale, box.y - region.y * scale, box) });
      if (t < 1) refitRef.current = requestAnimationFrame(step);
    };
    stopRefit();
    refitRef.current = requestAnimationFrame(step);
  };

  const zoomAt = (factor: number, px: number, py: number) => {
    if (!source || !frame) return;
    stopRefit();
    const { view, box } = frame;
    const minScale = box.side / Math.min(sourceWidth, sourceHeight);
    const scale = clamp(view.scale * factor, minScale, Math.max(minScale, maxScale, view.scale));
    const k = scale / view.scale;
    setFrame({ box, view: coverBox(scale, px - (px - view.x) * k, py - (py - view.y) * k, box) });
  };

  useEffect(() => {
    wheelRef.current = (e, rect) => zoomAt(Math.exp(-e.deltaY * 0.0015), e.clientX - rect.left, e.clientY - rect.top);
  });

  const viewportRef = useCallback((node: HTMLDivElement | null) => {
    if (!node) return;
    const listener = (e: WheelEvent) => {
      e.preventDefault();
      wheelRef.current?.(e, node.getBoundingClientRect());
    };
    node.addEventListener('wheel', listener, { passive: false });
    return () => node.removeEventListener('wheel', listener);
  }, []);

  const rotate = () => {
    if (!image) return;
    const next = (turns + 1) % 4;
    const [width, height] = sourceSize(image);
    stopRefit();
    setTurns(next);
    setFrame(next % 2 === 1 ? initialFrame(viewSide, height, width) : initialFrame(viewSide, width, height));
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!frame) return;
    const mode = (e.target as HTMLElement).closest<HTMLElement>('[data-mode]')?.dataset.mode as Drag['mode'] | undefined;
    if (!mode) return;
    e.preventDefault();
    stopRefit();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { mode, startX: e.clientX, startY: e.clientY, ...frame };
    setDragMode(mode);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || !source) return;
    if (e.buttons === 0) {
      endDrag();
      return;
    }
    const dx = e.clientX - drag.startX;
    const dy = e.clientY - drag.startY;
    const { view, box } = drag;
    if (drag.mode === 'pan') {
      setFrame({ box, view: coverBox(view.scale, view.x + dx, view.y + dy, box) });
      return;
    }
    const left = Math.max(VIEW_PAD, view.x);
    const top = Math.max(VIEW_PAD, view.y);
    const right = Math.min(viewSide - VIEW_PAD, view.x + sourceWidth * view.scale);
    const bottom = Math.min(viewSide - VIEW_PAD, view.y + sourceHeight * view.scale);
    const [sx, sy] = CORNER_SIGN[drag.mode];
    const ax = sx > 0 ? box.x : box.x + box.side;
    const ay = sy > 0 ? box.y : box.y + box.side;
    const maxSide = Math.min(sx > 0 ? right - ax : ax - left, sy > 0 ? bottom - ay : ay - top);
    const side = clamp(box.side + Math.max(sx * dx, sy * dy), Math.min(MIN_BOX_SIDE, maxSide), maxSide);
    setFrame({ view, box: { x: sx > 0 ? ax : ax - side, y: sy > 0 ? ay : ay - side, side } });
  };

  /** 松手、拖出窗口后松手（指针捕获丢失或回到窗口时按键已松开）都结束本次拖动 */
  const endDrag = () => {
    const drag = dragRef.current;
    if (!drag) return;
    dragRef.current = null;
    setDragMode(null);
    if (drag.mode !== 'pan' && frame && frame.box.side !== drag.box.side) refit(frame);
  };

  const confirm = async () => {
    if (!source || !frame) return;
    const { view, box } = frame;
    const canvas = document.createElement('canvas');
    canvas.width = OUTPUT_SIDE;
    canvas.height = OUTPUT_SIDE;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, OUTPUT_SIDE, OUTPUT_SIDE);
    ctx.imageSmoothingQuality = 'high';
    const side = box.side / view.scale;
    ctx.drawImage(source, (box.x - view.x) / view.scale, (box.y - view.y) / view.scale, side, side, 0, 0, OUTPUT_SIDE, OUTPUT_SIDE);
    const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, 'image/jpeg', 0.9));
    if (!blob) {
      setFailed(true);
      return;
    }
    setSaving(true);
    try {
      await onConfirm(blob);
    } finally {
      setSaving(false);
    }
  };

  const handle = (corner: Corner): React.CSSProperties => ({
    position: 'absolute',
    width: 18,
    height: 18,
    borderColor: '#fff',
    borderStyle: 'solid',
    borderWidth: 0,
    cursor: corner === 'nw' || corner === 'se' ? 'nwse-resize' : 'nesw-resize',
    ...(corner[0] === 'n' ? { top: -4, borderTopWidth: 4 } : { bottom: -4, borderBottomWidth: 4 }),
    ...(corner[1] === 'w' ? { left: -4, borderLeftWidth: 4 } : { right: -4, borderRightWidth: 4 }),
  });

  const box = frame?.box;

  return (
    <Modal
      open
      centered
      destroyOnHidden
      maskClosable={false}
      width={viewSide + 48}
      title={t('profile.avatar_crop_title', '剪裁和旋转')}
      onCancel={saving ? undefined : onCancel}
      footer={[
        <Button key="rotate" icon={<RotateRightOutlined />} onClick={rotate} disabled={!source || saving}>
          {t('profile.avatar_rotate', '旋转')}
        </Button>,
        <Button key="ok" type="primary" onClick={confirm} loading={saving} disabled={!source || !frame}>
          {t('profile.avatar_crop_done', '完成')}
        </Button>,
      ]}
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', minHeight: viewSide, background: '#141414', borderRadius: 8, overflow: 'hidden' }}>
        {failed ? (
          <span style={{ color: '#ff4d4f' }}>{t('profile.avatar_load_failed', '无法读取这张图片，请换一张试试')}</span>
        ) : (
          <div
            ref={viewportRef}
            data-mode="pan"
            style={{
              position: 'relative',
              width: viewSide,
              height: viewSide,
              overflow: 'hidden',
              touchAction: 'none',
              userSelect: 'none',
              cursor: dragMode === 'pan' ? 'grabbing' : 'grab',
            }}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
            onLostPointerCapture={endDrag}
          >
            <canvas ref={canvasRef} style={{ display: 'block', width: viewSide, height: viewSide }} />
            {box && (
              <div
                style={{
                  position: 'absolute',
                  left: box.x,
                  top: box.y,
                  width: box.side,
                  height: box.side,
                }}
              >
                <div style={{ position: 'absolute', inset: 0, borderRadius: '50%', boxShadow: '0 0 0 9999px rgba(0,0,0,0.5)', pointerEvents: 'none' }} />
                <div style={{ position: 'absolute', inset: 0, boxSizing: 'border-box', border: '2px solid #fff', pointerEvents: 'none' }} />
                {dragMode && GRID_LINES.map(line => <div key={line.key} style={line.style} />)}
                {(['nw', 'ne', 'sw', 'se'] as Corner[]).map(corner => (
                  <div key={corner} data-mode={corner} style={handle(corner)} />
                ))}
              </div>
            )}
          </div>
        )}
      </div>
      <Typography.Text type="secondary" style={{ display: 'block', marginTop: 12, textAlign: 'center', fontSize: 12 }}>
        {t('profile.avatar_crop_hint', '拖动图片调整位置，拖动四角调整剪裁框，滚轮缩放图片')}
      </Typography.Text>
    </Modal>
  );
};

export default AvatarCropModal;

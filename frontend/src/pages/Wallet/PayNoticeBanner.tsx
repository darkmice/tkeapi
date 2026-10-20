/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState } from 'react';
import { SoundOutlined, DownOutlined } from '@ant-design/icons';
import { Grid } from 'antd';
import { useTranslation } from 'react-i18next';
import request from '../../utils/request';
import { getAnnouncementLabel } from '../../utils/announcement';
import { useThemeStore } from '../../store/theme';

const { useBreakpoint } = Grid;

type PayNotice = {
  id: number;
  title: string;
  excerpt: string;
  html: string;
  pinned: boolean;
  expandable: boolean;
};

function toPlain(html: string): string {
  return html
    .replace(/<[^>]+>/g, ' ')
    .replace(/&nbsp;/gi, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&#39;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/\s+/g, ' ')
    .trim();
}

function buildNotice(raw: any): PayNotice | null {
  const title = toPlain(getAnnouncementLabel(String(raw?.title || '')));
  if (!title) return null;
  const html = getAnnouncementLabel(String(raw?.content || ''));
  const plain = toPlain(html);
  const sameAsTitle = !plain || plain === title;
  const excerpt = sameAsTitle ? '' : (plain.length > 72 ? `${plain.slice(0, 72)}…` : plain);
  return {
    id: Number(raw.id),
    title,
    excerpt,
    html: sameAsTitle ? '' : html,
    pinned: Number(raw.is_pinned) === 1,
    expandable: !sameAsTitle && (plain.length > 72 || /<(img|ul|ol|table|h[1-6]|pre|blockquote)\b/i.test(html)),
  };
}

const PayNoticeBanner: React.FC = () => {
  const { t } = useTranslation();
  const { themeMode } = useThemeStore();
  const isLight = themeMode === 'light';
  const screens = useBreakpoint();
  const isMobile = !screens.md;
  const [items, setItems] = useState<PayNotice[]>([]);
  const [openId, setOpenId] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    request
      .get('/announcements/public', {
        params: { module: 'pay_modal' },
        skipErrorHandler: true,
      } as any)
      .then((response: any) => {
        if (cancelled) return;
        const list = Array.isArray(response?.data) ? response.data : [];
        setItems(list.map(buildNotice).filter((item: PayNotice | null): item is PayNotice => !!item));
      })
      .catch(() => {
        if (!cancelled) setItems([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!items.length) return null;

  const many = items.length > 1;
  const tone = isLight
    ? {
        panel: '#fafafa',
        border: '#e4e4e7',
        title: '#18181b',
        muted: '#71717a',
        line: '#ececee',
        pin: '#b45309',
        pinBg: 'rgba(245, 158, 11, 0.14)',
        icon: '#b45309',
        iconBg: 'rgba(245, 158, 11, 0.14)',
        body: '#3f3f46',
      }
    : {
        panel: '#141417',
        border: '#27272a',
        title: '#fafafa',
        muted: '#a1a1aa',
        line: '#27272a',
        pin: '#fbbf24',
        pinBg: 'rgba(251, 191, 36, 0.12)',
        icon: '#fbbf24',
        iconBg: 'rgba(251, 191, 36, 0.12)',
        body: '#d4d4d8',
      };

  return (
    <div
      role="region"
      aria-label={t('recharge.pay_notice', '支付通知')}
      style={{
        marginTop: isMobile ? -2 : -8,
        marginBottom: isMobile ? 10 : 14,
        borderRadius: 12,
        border: `1px solid ${tone.border}`,
        background: tone.panel,
        overflow: 'hidden',
        position: 'relative',
      }}
    >
      <style>{`
        .pay-modal-notice-scroll::-webkit-scrollbar { width: 6px; }
        .pay-modal-notice-scroll::-webkit-scrollbar-thumb { background: rgba(161, 161, 170, 0.45); border-radius: 99px; }
        .pay-modal-notice-body p { margin: 0 0 6px; }
        .pay-modal-notice-body p:last-child { margin-bottom: 0; }
        .pay-modal-notice-body img { max-width: 100%; height: auto; border-radius: 8px; }
        .pay-modal-notice-body a { color: #d97706; }
        .pay-modal-notice-body ul, .pay-modal-notice-body ol { margin: 4px 0 0; padding-left: 18px; }
      `}</style>
      <span
        aria-hidden
        style={{
          position: 'absolute',
          left: 0,
          top: 0,
          bottom: 0,
          width: 3,
          background: 'linear-gradient(180deg, #fbbf24, #f59e0b)',
        }}
      />
      {many && (
        <div style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: isMobile ? '8px 12px 8px 14px' : '9px 14px 9px 16px',
          borderBottom: `1px solid ${tone.line}`,
        }}>
          <span style={{
            width: 22,
            height: 22,
            borderRadius: 7,
            display: 'inline-flex',
            alignItems: 'center',
            justifyContent: 'center',
            background: tone.iconBg,
            color: tone.icon,
            flexShrink: 0,
          }}>
            <SoundOutlined style={{ fontSize: 12 }} />
          </span>
          <span style={{ fontSize: 12, fontWeight: 600, color: tone.title, letterSpacing: 0.2 }}>
            {t('recharge.pay_notice', '支付通知')}
          </span>
          <span style={{
            marginLeft: 'auto',
            minWidth: 18,
            height: 18,
            padding: '0 6px',
            borderRadius: 999,
            background: tone.pinBg,
            color: tone.pin,
            fontSize: 11,
            fontWeight: 700,
            lineHeight: '18px',
            textAlign: 'center',
          }}>
            {items.length}
          </span>
        </div>
      )}
      <div
        className={items.length > 3 ? 'pay-modal-notice-scroll' : undefined}
        style={{
          maxHeight: items.length > 3 ? (isMobile ? 168 : 188) : undefined,
          overflowY: items.length > 3 ? 'auto' : 'visible',
        }}
      >
        {items.map((item, index) => {
          const open = openId === item.id;
          return (
            <div
              key={item.id}
              style={{
                borderTop: many && index > 0 ? `1px solid ${tone.line}` : undefined,
                padding: isMobile ? '8px 12px 8px 14px' : '9px 14px 9px 16px',
              }}
            >
              <button
                type="button"
                aria-expanded={item.expandable ? open : undefined}
                onClick={() => {
                  if (!item.expandable) return;
                  setOpenId(open ? null : item.id);
                }}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  width: '100%',
                  padding: 0,
                  border: 'none',
                  background: 'transparent',
                  cursor: item.expandable ? 'pointer' : 'default',
                  textAlign: 'left',
                }}
              >
                {!many && (
                  <span style={{
                    width: 22,
                    height: 22,
                    borderRadius: 7,
                    display: 'inline-flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    background: tone.iconBg,
                    color: tone.icon,
                    flexShrink: 0,
                  }}>
                    <SoundOutlined style={{ fontSize: 12 }} />
                  </span>
                )}
                <span style={{
                  flex: 1,
                  minWidth: 0,
                  fontSize: isMobile ? 12 : 13,
                  fontWeight: 600,
                  color: tone.title,
                  lineHeight: 1.35,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}>
                  {item.title}
                </span>
                {item.pinned && (
                  <span style={{
                    flexShrink: 0,
                    fontSize: 10,
                    fontWeight: 700,
                    lineHeight: '16px',
                    padding: '0 6px',
                    borderRadius: 999,
                    color: tone.pin,
                    background: tone.pinBg,
                  }}>
                    {t('recharge.pay_notice_pinned', '置顶')}
                  </span>
                )}
                {item.expandable && (
                  <DownOutlined
                    style={{
                      flexShrink: 0,
                      fontSize: 10,
                      color: tone.muted,
                      transform: open ? 'rotate(180deg)' : undefined,
                      transition: 'transform 0.2s ease',
                    }}
                  />
                )}
              </button>
              {!open && !!item.excerpt && (
                <div style={{
                  marginTop: 3,
                  marginLeft: many ? 0 : 30,
                  fontSize: 12,
                  lineHeight: 1.45,
                  color: tone.muted,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: many ? 'nowrap' : 'normal',
                }}>
                  {item.excerpt}
                </div>
              )}
              {open && !!item.html && (
                <div
                  className="pay-modal-notice-body"
                  style={{
                    marginTop: 8,
                    marginLeft: many ? 0 : 30,
                    maxHeight: 140,
                    overflowY: 'auto',
                    fontSize: 12,
                    lineHeight: 1.55,
                    color: tone.body,
                  }}
                  dangerouslySetInnerHTML={{ __html: item.html }}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default PayNoticeBanner;

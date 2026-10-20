/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect } from 'react';
import { Typography, Input, InputNumber, Switch, Button, Divider, Spin, App, Space, Tag, Alert, Tooltip } from 'antd';
import { SaveOutlined, EyeOutlined, ThunderboltOutlined, PlusOutlined, DeleteOutlined, LinkOutlined, CopyOutlined } from '@ant-design/icons';
import { LayoutDashboard, Code, ShieldCheck, PanelBottom, FileCode, Home, Mail, Info, Share2 } from 'lucide-react';
import request from '../../../utils/request';
import { useThemeStore } from '../../../store/theme';
import { copyWithFeedback } from '../../../utils/clipboard';
import whatsTokenHomepageHtml from './whats-token-homepage.html?raw';

const { Text, Title } = Typography;
const { TextArea } = Input;

class ErrorBoundary extends React.Component<{ children: React.ReactNode }, { hasError: boolean; error: any }> {
  constructor(props: any) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: any) {
    return { hasError: true, error };
  }

  componentDidCatch(error: any, errorInfo: any) {
    console.error("ErrorBoundary caught an error", error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div style={{ padding: 20, background: '#fff1f0', border: '1px solid #ffa39e', borderRadius: 8 }}>
          <Title level={5} style={{ color: '#ff4d4f', marginTop: 0 }}>配置面板加载失败</Title>
          <Text type="secondary" style={{ fontSize: 13, display: 'block', marginBottom: 12 }}>
            渲染该配置栏目时发生运行时错误。这通常是由于旧配置数据结构不兼容导致的，您可以点击下方按钮重试，或联系管理员排查。
          </Text>
          <pre style={{
            background: '#fafafa',
            padding: 12,
            borderRadius: 5,
            border: '1px solid rgba(0,0,0,0.06)',
            color: '#ff4d4f',
            fontFamily: 'monospace',
            fontSize: 12,
            overflowX: 'auto',
            maxHeight: 250
          }}>
            {this.state.error?.stack || this.state.error?.toString()}
          </pre>
          <Button size="small" type="primary" danger onClick={() => this.setState({ hasError: false, error: null })}>
            重试
          </Button>
        </div>
      );
    }
    return this.props.children;
  }
}

const DEMO_HTML = whatsTokenHomepageHtml;

const FIXED_NAV_KEYS = ['home', 'marketplace', 'contact', 'about'] as const;
const SYSTEM_MARKETPLACE_PATH = '/home/models';

const DEFAULT_NAV_ITEMS = [
  { label: '站点首页|Home', path: '/home', enabled: true, target_blank: false, key: 'home', is_fixed: true, sort: 40 },
  { label: '模型广场|Model Marketplace', path: SYSTEM_MARKETPLACE_PATH, enabled: true, target_blank: false, key: 'marketplace', is_fixed: true, sort: 30 },
  { label: '联系我们|Contact Us', path: '/home/contact', enabled: true, target_blank: false, key: 'contact', is_fixed: true, sort: 20 },
  { label: '关于我们|About Us', path: '/home/about', enabled: true, target_blank: false, key: 'about', is_fixed: true, sort: 10 },
];

function isFixedNavItem(item: any): boolean {
  if (!item) return false;
  if (item.is_fixed === true) return true;
  return (FIXED_NAV_KEYS as readonly string[]).includes(item.key);
}

function defaultNavItemByKey(key: (typeof FIXED_NAV_KEYS)[number]) {
  return DEFAULT_NAV_ITEMS.find(item => item.key === key)!;
}

function parseSortValue(value: unknown, fallback: number): number {
  const n = Number(value);
  return Number.isFinite(n) ? n : fallback;
}

function withDefaultSort<T extends { sort?: unknown }>(items: T[]): (T & { sort: number })[] {
  const total = items.length;
  return items.map((item, idx) => ({
    ...item,
    sort: parseSortValue(item.sort, (total - idx) * 10),
  }));
}

function sortByWeightDesc<T extends { sort?: unknown }>(items: T[]): T[] {
  return items
    .map((item, idx) => ({ item, idx }))
    .sort((a, b) => {
      const wa = parseSortValue(a.item.sort, 0);
      const wb = parseSortValue(b.item.sort, 0);
      if (wb !== wa) return wb - wa;
      return a.idx - b.idx;
    })
    .map(({ item }) => item);
}

function rankedList<T extends { sort?: unknown }>(items: T[]): (T & { sort: number })[] {
  return sortByWeightDesc(withDefaultSort(items));
}

function isDeadClassicAnchor(item: any): boolean {
  const key = String(item?.key || '');
  const path = String(item?.path || '').trim();
  const dead = path === '' || path.startsWith('#');
  return (key === 'integration' || key === 'features' || key === 'carousel') && dead;
}

/** 保证系统四项存在且不可删除；按 sort 数值越大越靠前。模型广场默认进系统主页面。 */
function normalizeNavItems(rawItems: any[] = []): any[] {
  const items = (Array.isArray(rawItems) ? rawItems : []).filter(it => !isDeadClassicAnchor(it));
  const marked = items.map(it => {
    const next = isFixedNavItem(it) ? { ...it, is_fixed: true } : { ...it };
    if (next.key === 'marketplace') {
      const path = String(next.path || '').trim();
      if (!path || path === '#models' || path === '/portal/models' || path === '/portal/models/') {
        next.path = SYSTEM_MARKETPLACE_PATH;
      }
    }
    return next;
  });
  for (const key of FIXED_NAV_KEYS) {
    if (!marked.some(it => it.key === key)) {
      marked.push({ ...defaultNavItemByKey(key), sort: 0 });
    }
  }
  return rankedList(marked);
}

/** 导航预览：相对路径补 origin，锚点拼到首页 logo_link。 */
function resolveNavPreviewUrl(path: string | undefined, logoLink?: string): string {
  const raw = (path || '').trim();
  if (!raw) return '';
  if (/^https?:\/\//i.test(raw)) return raw;
  if (raw.startsWith('#')) {
    const base = (logoLink || '/home').trim() || '/home';
    if (/^https?:\/\//i.test(base)) {
      return `${base.replace(/\/$/, '')}${raw}`;
    }
    const basePath = base.startsWith('/') ? base : `/${base}`;
    return `${window.location.origin}${basePath}${raw}`;
  }
  if (raw.startsWith('/')) return `${window.location.origin}${raw}`;
  return `${window.location.origin}/${raw}`;
}

const DEFAULT_ABOUT = {
  title: '关于我们',
  path: 'about',
  enabled: true,
  hero_subtitle: '作为业内领先的 AI 模型聚合服务平台，我们消除服务商锁定风险，通过更优的价格、高可用性和企业级的稳定性，为您打通全球最先进大语言模型的访问桥梁。',
  stat_tokens: '10B+',
  stat_developers: '10K+',
  stat_providers: '40+',
  stat_models: '200+',
  press_title: '统一大模型接口体验，让接入 AI 零负担',
  press_desc: '旨在成为开发者接入、对比、调用不同 AI 模型的首选聚合网关。只需一次接入，即可全量访问数百个前沿模型。',
  press_link_text: '联系我们洽谈合作 →',
  content: '',
};

const DEFAULT_CONTACT_SOCIAL_LINKS = [
  { platform: 'Twitter / X', value: 'https://x.com/tkeapi', enabled: true, sort: 30, key: 'social_twitter' },
  { platform: 'GitHub', value: 'https://github.com/aiqachat/tkeapi', enabled: true, sort: 20, key: 'social_github' },
  { platform: 'Telegram', value: 'https://t.me/tokensbyte', enabled: true, sort: 10, key: 'social_telegram' },
];

const DEFAULT_CHANNEL_ORDER = ['email', 'phone', 'address'] as const;

const CONTACT_CHANNEL_FIELDS: { key: (typeof DEFAULT_CHANNEL_ORDER)[number]; label: string; placeholder: string }[] = [
  { key: 'email', label: '官方邮箱 (Email)', placeholder: '例如：contact@tokensbyte.ai' },
  { key: 'phone', label: '联系电话 (Phone)', placeholder: '例如：+1 (800) 123-4567' },
  { key: 'address', label: '办公地址 (Address)', placeholder: '例如：30 N Gould St Ste R, Sheridan, WY 82801' },
];

const DEFAULT_CONTACT = {
  title: '联系我们',
  path: 'contact',
  enabled: true,
  content: {
    email: '',
    phone: '',
    address: '',
    email_enabled: true,
    phone_enabled: true,
    address_enabled: true,
    channel_order: [...DEFAULT_CHANNEL_ORDER],
    channel_sort: { email: 30, phone: 20, address: 10 },
    social_links: DEFAULT_CONTACT_SOCIAL_LINKS.map((item) => ({ ...item })),
    custom_content: '',
    items: [] as { icon?: string; title?: string; value?: string; enabled?: boolean }[],
  },
};

function normalizeChannelOrder(order: any): string[] {
  const allowed = new Set<string>(DEFAULT_CHANNEL_ORDER);
  const fromCfg = Array.isArray(order) ? order.filter((key) => allowed.has(String(key))) : [];
  const seen = new Set(fromCfg.map(String));
  return [...fromCfg.map(String), ...DEFAULT_CHANNEL_ORDER.filter((key) => !seen.has(key))];
}

function normalizeChannelSort(content: any): Record<string, number> {
  const order = normalizeChannelOrder(content?.channel_order);
  const raw = content?.channel_sort && typeof content.channel_sort === 'object' && !Array.isArray(content.channel_sort)
    ? content.channel_sort
    : {};
  const out: Record<string, number> = {};
  order.forEach((key, idx) => {
    out[key] = parseSortValue(raw[key], (order.length - idx) * 10);
  });
  return out;
}

function channelOrderFromSort(sort: Record<string, number>): string[] {
  return [...DEFAULT_CHANNEL_ORDER].sort((a, b) => {
    const diff = (sort[b] ?? 0) - (sort[a] ?? 0);
    if (diff !== 0) return diff;
    return DEFAULT_CHANNEL_ORDER.indexOf(a) - DEFAULT_CHANNEL_ORDER.indexOf(b);
  });
}

function normalizeAbout(ab: any) {
  return { ...DEFAULT_ABOUT, ...(ab || {}), enabled: ab?.enabled ?? true };
}

function hasUsableSocialLinks(links: any): boolean {
  return Array.isArray(links) && links.some((item) => {
    const platform = String(item?.platform || '').trim();
    const value = String(item?.value || '').trim();
    return !!(platform || value);
  });
}

function defaultSocialLinks() {
  return DEFAULT_CONTACT_SOCIAL_LINKS.map((item) => ({ ...item }));
}

function normalizeSocialLinks(links: any[]) {
  return rankedList(links.map((item, idx) => ({
    ...item,
    key: item.key || `social_${idx}`,
    value: String(item?.value || '').trim() === 'https://x.com/tokensbyte'
      ? 'https://x.com/tkeapi'
      : item?.value,
    enabled: item.enabled !== false,
  })));
}

function itemValueByTitle(items: any[], needles: string[]): string {
  const hit = items.find((item) => needles.some((n) => String(item?.title || '').includes(n)));
  return (hit?.value || '').trim();
}

function normalizeContact(ct: any) {
  const cnt = ct?.content || {};
  const items = Array.isArray(cnt.items) ? cnt.items : [];
  const channel_sort = normalizeChannelSort(cnt);
  return {
    ...DEFAULT_CONTACT,
    ...(ct || {}),
    enabled: ct?.enabled ?? true,
    content: {
      email: cnt.email || itemValueByTitle(items, ['邮箱', 'mail', 'Mail']) || '',
      phone: cnt.phone || itemValueByTitle(items, ['电话', 'tel', 'Tel']) || '',
      address: cnt.address || itemValueByTitle(items, ['地址', 'addr']) || '',
      email_enabled: cnt.email_enabled !== false,
      phone_enabled: cnt.phone_enabled !== false,
      address_enabled: cnt.address_enabled !== false,
      channel_sort,
      channel_order: channelOrderFromSort(channel_sort),
      social_links: normalizeSocialLinks(
        hasUsableSocialLinks(cnt.social_links) ? cnt.social_links : defaultSocialLinks(),
      ),
      custom_content: cnt.custom_content || '',
      items,
    },
  };
}

function contactSavePayload(cfg: any) {
  const content = { ...(cfg.content || {}) };
  const channel_sort = normalizeChannelSort(content);
  const channel_order = channelOrderFromSort(channel_sort);
  const social_links = rankedList(Array.isArray(content.social_links) ? content.social_links : []);
  const items = Array.isArray(content.items) ? content.items.map((item: any) => ({ ...item })) : [];
  const sync = (needles: string[], value: string, enabled: boolean) => {
    const idx = items.findIndex((item: any) => needles.some((n) => String(item?.title || '').includes(n)));
    if (idx >= 0) items[idx] = { ...items[idx], value, enabled };
  };
  sync(['邮箱', 'mail', 'Mail'], content.email || '', content.email_enabled !== false);
  sync(['电话', 'tel', 'Tel'], content.phone || '', content.phone_enabled !== false);
  sync(['地址', 'addr'], content.address || '', content.address_enabled !== false);
  const needlesByKey: Record<string, string[]> = {
    email: ['邮箱', 'mail', 'Mail'],
    phone: ['电话', 'tel', 'Tel'],
    address: ['地址', 'addr'],
  };
  const used = new Set<number>();
  const orderedItems: any[] = [];
  for (const key of channel_order) {
    const needles = needlesByKey[key] || [];
    const idx = items.findIndex((item: any, i: number) => !used.has(i) && needles.some((n) => String(item?.title || '').includes(n)));
    if (idx >= 0) {
      used.add(idx);
      orderedItems.push(items[idx]);
    }
  }
  items.forEach((item: any, idx: number) => {
    if (!used.has(idx)) orderedItems.push(item);
  });
  return { ...cfg, content: { ...content, channel_sort, channel_order, social_links, items: orderedItems } };
}

function SortInput({ value, onChange, onCommit }: { value: number; onChange: (n: number) => void; onCommit?: () => void }) {
  return (
    <InputNumber
      size="small"
      value={value}
      onChange={(v) => onChange(parseSortValue(v, 0))}
      onBlur={onCommit}
      style={{ width: 72 }}
      title="数值越大越靠前"
    />
  );
}

function portalPagePreviewPath(slug: string | undefined, fallback: string): string {
  const raw = (slug || fallback).trim().replace(/^\/+|\/+$/g, '') || fallback;
  return `/home/${raw}`;
}

type MenuKey = 'custom_homepage' | 'home' | 'contact' | 'about' | 'nav' | 'static_gen' | 'other' | 'footer';

const PortalManager: React.FC = () => {
  const { themeMode } = useThemeStore();
  const _isLight = themeMode === 'light';
  const { message } = App.useApp();
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [activeMenu, setActiveMenu] = useState<MenuKey>('nav');
  const [saveCooldowns, setSaveCooldowns] = useState<Record<string, number>>({});
  // Config states
  const [navConfig, setNavConfig] = useState<any>({});
  const [footerConfig, setFooterConfig] = useState<any>({});
  const [customScripts, setCustomScripts] = useState<any>({});
  const [seoConfig, setSeoConfig] = useState<any>({});
  const [staticGenConfig, setStaticGenConfig] = useState<any>({ manual_mode: false });
  const [generateLog, setGenerateLog] = useState<any[]>([]);
  const [generating, setGenerating] = useState(false);
  const [generatedLinks, setGeneratedLinks] = useState<{ label: string; path: string }[]>([]);
  const [customHomepage, setCustomHomepage] = useState<any>({ enabled: false, html: '' });
  const [homeConfig, setHomeConfig] = useState<any>({ api_base_url: '' });
  const [aboutConfig, setAboutConfig] = useState<any>(DEFAULT_ABOUT);
  const [contactConfig, setContactConfig] = useState<any>(DEFAULT_CONTACT);

  useEffect(() => { fetchConfig(); }, []);

  useEffect(() => {
    setContactConfig((prev: any) => {
      if (hasUsableSocialLinks(prev?.content?.social_links)) return prev;
      return {
        ...prev,
        content: {
          ...(prev.content || {}),
          social_links: defaultSocialLinks(),
        },
      };
    });
  }, []);

  useEffect(() => {
    const timer = setInterval(() => {
      setSaveCooldowns(prev => {
        const next = { ...prev };
        let changed = false;
        for (const key in next) {
          if (next[key] > 0) {
            next[key] -= 1;
            changed = true;
          } else {
            delete next[key];
            changed = true;
          }
        }
        return changed ? next : prev;
      });
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  const fetchConfig = async (opts?: { silent?: boolean }) => {
    const silent = opts?.silent === true;
    try {
      if (!silent) setLoading(true);
      const res = await (request.get('/plugins/site-portal/portal-config') as Promise<any>);
      if (res.nav_config) {
        setNavConfig({
          ...res.nav_config,
          items: normalizeNavItems(res.nav_config.items || DEFAULT_NAV_ITEMS),
        });
      }
      if (res.footer_config) setFooterConfig(res.footer_config);
      if (res.custom_scripts) setCustomScripts(res.custom_scripts);
      if (res.seo_config) setSeoConfig(res.seo_config);
      if (res.static_gen_config) setStaticGenConfig(res.static_gen_config);
      if (res.generate_log) setGenerateLog(res.generate_log);
      if (res.custom_homepage) {
        setCustomHomepage(res.custom_homepage);
        if (!silent && res.custom_homepage.enabled) {
          setActiveMenu('custom_homepage');
        }
      }
      if (res.home_config) setHomeConfig(res.home_config);
      if (res.columns?.about) setAboutConfig(normalizeAbout(res.columns.about));
      setContactConfig(normalizeContact(res.columns?.contact));
    } catch (e) {
      console.error(e);
    } finally {
      if (!silent) setLoading(false);
    }
  };
  const handleSave = async (section: string, data: any) => {
    if (saveCooldowns[section]) return;
    try {
      setSaving(true);
      await request.post('/plugins/site-portal/portal-config', { section, data });
      setSaveCooldowns(prev => ({ ...prev, [section]: 3 }));
      message.success('配置已保存');
    } catch (e) {
      console.error(e);
    } finally {
      setSaving(false);
    }
  };

  const handleSaveOther = async () => {
    if (saveCooldowns['scripts']) return;
    try {
      setSaving(true);
      await request.post('/plugins/site-portal/portal-config', { section: 'home', data: { api_base_url: (homeConfig.api_base_url || '').trim() } });
      await request.post('/plugins/site-portal/portal-config', { section: 'scripts', data: customScripts });
      setSaveCooldowns(prev => ({ ...prev, scripts: 3 }));
      message.success('配置已保存');
    } catch (e) {
      console.error(e);
    } finally {
      setSaving(false);
    }
  };

  const handleSaveAllNav = async () => {
    if (saveCooldowns['nav']) return;
    try {
      setSaving(true);
      await request.post('/plugins/site-portal/portal-config', {
        section: 'nav',
        data: { ...navConfig, items: normalizeNavItems(navConfig.items || DEFAULT_NAV_ITEMS) },
      });
      await request.post('/plugins/site-portal/portal-config', { section: 'seo', data: seoConfig });
      setSaveCooldowns(prev => ({ ...prev, 'nav': 3 }));
      message.success('导航配置已保存');
    } catch (e) {
      console.error(e);
    } finally {
      setSaving(false);
    }
  };

  const handleGenerate = async (scope: string, columns?: string[]) => {
    try {
      setGenerating(true);
      setGeneratedLinks([]);
      const res = await (request.post('/plugins/site-portal/generate', { scope, columns }) as Promise<any>);
      message.success(res.message || '生成完成');
      if (res.generated_paths && Array.isArray(res.generated_paths)) {
        setGeneratedLinks(res.generated_paths);
      }
      fetchConfig();
    } catch (err: any) {
      console.error(err);
    } finally {
      setGenerating(false);
    }
  };

  const cardStyle = {
    background: _isLight ? '#fff' : '#141414',
    borderRadius: 8,
    border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
    padding: '20px',
    marginBottom: 16,
  };

  const labelStyle = { color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13, display: 'block' as const, marginBottom: 6 };

  // ─── Left Menu ───
  const menuItems: { key: MenuKey; icon?: React.ReactNode; label: string; isTitle?: boolean; isSub?: boolean }[] = [
    { key: 'custom_homepage', icon: <FileCode size={16} strokeWidth={1.5} />, label: '自定义主页' },
    { key: 'home', icon: <Home size={16} strokeWidth={1.5} />, label: '首页配置', isSub: true },
    { key: 'contact', icon: <Mail size={16} strokeWidth={1.5} />, label: '联系我们', isSub: true },
    { key: 'about', icon: <Info size={16} strokeWidth={1.5} />, label: '关于我们', isSub: true },
    { key: 'nav', icon: <LayoutDashboard size={16} strokeWidth={1.5} />, label: '导航管理' },
    { key: 'footer', icon: <PanelBottom size={16} strokeWidth={1.5} />, label: '底部管理' },
    { key: 'other', icon: <ShieldCheck size={16} strokeWidth={1.5} />, label: '其他配置' },
    { key: 'static_gen', icon: <Code size={16} strokeWidth={1.5} />, label: '静态生成' },
  ];

  // ─── Right Panel Content ───

  const renderNav = () => (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0, color: _isLight ? '#1f2937' : '#fff' }}>导航管理</Title>
        <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={(saveCooldowns['nav'] || 0) > 0} onClick={handleSaveAllNav}>
          {(saveCooldowns['nav'] || 0) > 0 ? `已保存 (${saveCooldowns['nav']}s)` : '保存导航配置'}
        </Button>
      </div>
      <div style={cardStyle}>
        <Text style={labelStyle}>Logo 图片 URL（留空则只显示文字）</Text>
        <Input value={navConfig.logo_url || ''} onChange={e => setNavConfig({ ...navConfig, logo_url: e.target.value })} placeholder="https://cdn.example.com/logo.png" style={{ marginBottom: 12 }} />
        <Text style={labelStyle}>Logo 点击跳转链接</Text>
        <Input value={navConfig.logo_link || ''} onChange={e => setNavConfig({ ...navConfig, logo_link: e.target.value })} placeholder="例如：/home 或 https://..." style={{ marginBottom: 12 }} />
        <Text style={labelStyle}>Logo 文字</Text>
        <Input value={navConfig.logo_text || ''} onChange={e => setNavConfig({ ...navConfig, logo_text: e.target.value })} placeholder="Tkeapi" style={{ marginBottom: 12 }} />
        <Divider style={{ borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)', margin: '14px 0' }} />
        <Text style={labelStyle}>登录按钮文字 / 链接</Text>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8, marginBottom: 12 }}>
          <Input value={navConfig.cta_text || '登录'} onChange={e => setNavConfig({ ...navConfig, cta_text: e.target.value })} />
          <Input value={navConfig.cta_link || '/login'} onChange={e => setNavConfig({ ...navConfig, cta_link: e.target.value })} />
        </div>
        <Text style={labelStyle}>注册按钮文字 / 链接</Text>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
          <Input value={navConfig.register_text || '注册'} onChange={e => setNavConfig({ ...navConfig, register_text: e.target.value })} />
          <Input value={navConfig.register_link || '/register'} onChange={e => setNavConfig({ ...navConfig, register_link: e.target.value })} />
        </div>
      </div>

      {/* 顶部导航菜单 */}
      <div style={cardStyle}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 4 }}>
          <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>顶部导航菜单</Text>
          <Space size={8}>
            <Button size="small" onClick={() => {
              setNavConfig({ ...navConfig, items: DEFAULT_NAV_ITEMS.map(item => ({ ...item })) });
            }}>恢复系统默认</Button>
            <Button size="small" icon={<PlusOutlined />} onClick={() => {
              const items = [...(navConfig.items || []), { label: '新菜单|New Menu', path: '#features', enabled: true, target_blank: false, key: `item_${Date.now()}`, sort: 0 }];
              setNavConfig({ ...navConfig, items });
            }}>添加栏目</Button>
          </Space>
        </div>
        <div style={{ marginBottom: 12 }}>
          <Text type="secondary" style={{ fontSize: 13 }}>💡 提示：系统内置「站点首页、模型广场、联系我们、关于我们」不可删除，可调整排序、开启关闭、修改名称和链接路径。模型广场默认链接系统主页面 <Text code>/home/models</Text>。排序数值越大越靠前。自定义栏目可增删。名称格式为 <Text code>中文|English</Text>。</Text>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 16, marginBottom: 16, padding: '10px 12px', borderRadius: 8, background: _isLight ? 'rgba(0,0,0,0.03)' : 'rgba(255,255,255,0.04)' }}>
          <div>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13, display: 'block' }}>顶部模型搜索框</Text>
            <Text type="secondary" style={{ fontSize: 12 }}>开启后在顶部导航显示搜索框，可按模型名称、ID、MID 搜索并跳转模型广场</Text>
          </div>
          <Switch
            checked={navConfig.show_model_search !== false}
            onChange={v => setNavConfig({ ...navConfig, show_model_search: v })}
          />
        </div>
        {(navConfig.items || []).length > 0 && (
          <div style={{ display: 'grid', gridTemplateColumns: '72px 36px 1.2fr 1.5fr 1fr 88px 64px', gap: 8, marginBottom: 8, padding: '0 2px', alignItems: 'center' }}>
            <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>排序</Text>
            <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>启用</Text>
            <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>菜单名称</Text>
            <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>链接 / 路径</Text>
            <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>图标 SVG (可选)</Text>
            <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>新窗口</Text>
            <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', textAlign: 'right' }}>操作</Text>
          </div>
        )}
        {(navConfig.items || []).map((item: any, idx: number) => (
          <div key={item.key || idx} style={{ display: 'grid', gridTemplateColumns: '72px 36px 1.2fr 1.5fr 1fr 88px 64px', gap: 8, marginBottom: 8, alignItems: 'center' }}>
            <SortInput
              value={parseSortValue(item.sort, 0)}
              onChange={(sort) => {
                const items = [...navConfig.items];
                items[idx] = { ...item, sort };
                setNavConfig({ ...navConfig, items });
              }}
              onCommit={() => setNavConfig((prev: any) => ({ ...prev, items: rankedList(prev.items || []) }))}
            />
            <Tooltip title={item.enabled !== false ? '已启用（点击禁用）' : '已禁用（点击启用）'}>
              <Switch
                size="small"
                checked={item.enabled !== false}
                onChange={v => {
                  const items = [...navConfig.items];
                  items[idx] = { ...item, enabled: v };
                  setNavConfig({ ...navConfig, items });
                }}
              />
            </Tooltip>
            <Input
              value={item.label}
              onChange={e => {
                const items = [...navConfig.items];
                items[idx] = { ...item, label: e.target.value };
                setNavConfig({ ...navConfig, items });
              }}
              placeholder="菜单名称（如：联系我们|Contact Us）"
            />
            <Input
              value={item.path}
              onChange={e => {
                const items = [...navConfig.items];
                items[idx] = { ...item, path: e.target.value };
                setNavConfig({ ...navConfig, items });
              }}
              placeholder="链接（如 /home、/home/models、/home/contact）"
            />
            <Input
              value={item.icon || ''}
              onChange={e => {
                const items = [...navConfig.items];
                items[idx] = { ...item, icon: e.target.value };
                setNavConfig({ ...navConfig, items });
              }}
              placeholder="图标 SVG（风格化页）"
            />
            <Tooltip title="开启后在前台点击该菜单将在新窗口/新标签页中打开">
              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <Switch
                  size="small"
                  checked={!!item.target_blank}
                  onChange={v => {
                    const items = [...navConfig.items];
                    items[idx] = { ...item, target_blank: v };
                    setNavConfig({ ...navConfig, items });
                  }}
                />
                <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', whiteSpace: 'nowrap' }}>新窗口</Text>
              </div>
            </Tooltip>
            <Space size={4} style={{ justifyContent: 'flex-end' }}>
              <Button
                size="small"
                icon={<EyeOutlined />}
                title="预览"
                onClick={() => {
                  const url = resolveNavPreviewUrl(item.path, navConfig.logo_link);
                  if (!url) {
                    message.warning('请先填写链接');
                    return;
                  }
                  window.open(url, '_blank', 'noopener,noreferrer');
                }}
              />
              <Button
                size="small"
                danger
                icon={<DeleteOutlined />}
                disabled={isFixedNavItem(item)}
                title={isFixedNavItem(item) ? '系统内置菜单不可删除' : '删除'}
                onClick={() => {
                  if (isFixedNavItem(item)) return;
                  const items = navConfig.items.filter((_: any, i: number) => i !== idx);
                  setNavConfig({ ...navConfig, items });
                }}
              />
            </Space>
          </div>
        ))}
        {(!navConfig.items || navConfig.items.length === 0) && (
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)', fontSize: 13 }}>暂无导航菜单，点击添加</Text>
        )}
      </div>
      {/* SEO */}
      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>SEO 元信息</Text>
        <Text style={labelStyle}>页面标题 (meta title)</Text>
        <Input value={seoConfig.meta_title || ''} onChange={e => setSeoConfig({ ...seoConfig, meta_title: e.target.value })} placeholder="站点标题" style={{ marginBottom: 8 }} />
        <Text style={labelStyle}>页面描述 (meta description)</Text>
        <Input value={seoConfig.meta_description || ''} onChange={e => setSeoConfig({ ...seoConfig, meta_description: e.target.value })} placeholder="站点描述" style={{ marginBottom: 8 }} />
        <Text style={labelStyle}>关键词 (meta keywords)</Text>
        <Input value={seoConfig.meta_keywords || ''} onChange={e => setSeoConfig({ ...seoConfig, meta_keywords: e.target.value })} placeholder="AI, API, 模型" />
      </div>
    </div>
  );

  const renderFooter = () => (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0, color: _isLight ? '#1f2937' : '#fff' }}>底部管理</Title>
        <Space>
          <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={(saveCooldowns['footer'] || 0) > 0} onClick={() => handleSave('footer', footerConfig)}>
            {(saveCooldowns['footer'] || 0) > 0 ? `已保存 (${saveCooldowns['footer']}s)` : '保存'}
          </Button>
        </Space>
      </div>
      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>品牌与简介</Text>
        <Text style={labelStyle}>底部品牌名称（支持 中文|English）</Text>
        <Input value={footerConfig.brand_name || ''} onChange={e => setFooterConfig({ ...footerConfig, brand_name: e.target.value })} placeholder="Tkeapi" style={{ marginBottom: 8 }} />
        <Text style={labelStyle}>品牌简介（支持 中文|English）</Text>
        <Input.TextArea value={footerConfig.description || ''} onChange={e => setFooterConfig({ ...footerConfig, description: e.target.value })} rows={3} style={{ marginBottom: 8 }} />
        <Divider style={{ margin: '16px 0' }} />
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>产品与服务</Text>
        <Text style={labelStyle}>栏目标题（支持 中文|English）</Text>
        <Input value={footerConfig.links_title || ''} onChange={e => setFooterConfig({ ...footerConfig, links_title: e.target.value })} placeholder="产品与服务|Products & Services" style={{ marginBottom: 8 }} />
        <Text style={labelStyle}>底部链接列表（前台开启/关闭控制）</Text>
        {(footerConfig.links || []).map((link: any, idx: number) => (
          <div key={idx} style={{ display: 'grid', gridTemplateColumns: 'auto 1fr 1fr 32px', gap: 8, marginBottom: 8, alignItems: 'center' }}>
            <Switch
              size="small"
              checked={link.enabled !== false}
              onChange={v => {
                const links = [...(footerConfig.links || [])];
                links[idx] = { ...link, enabled: v };
                setFooterConfig({ ...footerConfig, links });
              }}
            />
            <Input value={link.label || ''} onChange={e => { const links = [...(footerConfig.links || [])]; links[idx] = { ...link, label: e.target.value }; setFooterConfig({ ...footerConfig, links }); }} placeholder="名称，如 技术优势|Technical Advantages" />
            <Input value={link.path || ''} onChange={e => { const links = [...(footerConfig.links || [])]; links[idx] = { ...link, path: e.target.value }; setFooterConfig({ ...footerConfig, links }); }} placeholder="链接，如 /home/models" />
            <Button danger icon={<DeleteOutlined />} onClick={() => setFooterConfig({ ...footerConfig, links: (footerConfig.links || []).filter((_: any, i: number) => i !== idx) })} />
          </div>
        ))}
        <Button size="small" icon={<PlusOutlined />} onClick={() => setFooterConfig({ ...footerConfig, links: [...(footerConfig.links || []), { label: '新链接|New Link', path: '/home', enabled: true }] })}>添加底部链接</Button>
        <Divider style={{ margin: '16px 0' }} />
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>开发者资讯</Text>
        <Text style={labelStyle}>栏目标题（支持 中文|English）</Text>
        <Input value={footerConfig.news_title || ''} onChange={e => setFooterConfig({ ...footerConfig, news_title: e.target.value })} placeholder="开发者资讯|Developer News" style={{ marginBottom: 8 }} />
        <Text style={labelStyle}>栏目说明（支持 中文|English）</Text>
        <Input.TextArea value={footerConfig.news_description || ''} onChange={e => setFooterConfig({ ...footerConfig, news_description: e.target.value })} rows={3} />
        <Divider style={{ margin: '16px 0' }} />
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>公司与法律信息</Text>
        <Text style={labelStyle}>版权信息</Text>
        <Input value={footerConfig.copyright || ''} onChange={e => setFooterConfig({ ...footerConfig, copyright: e.target.value })} placeholder="© 2026 TkeAPI. All rights reserved." style={{ marginBottom: 8 }} />
        <Text style={labelStyle}>公司名称</Text>
        <Input value={footerConfig.company_name || ''} onChange={e => setFooterConfig({ ...footerConfig, company_name: e.target.value })} style={{ marginBottom: 8 }} />
        <Text style={labelStyle}>公司地址</Text>
        <Input value={footerConfig.company_address || ''} onChange={e => setFooterConfig({ ...footerConfig, company_address: e.target.value })} style={{ marginBottom: 8 }} />
        <Text style={labelStyle}>备案号（可选）</Text>
        <Input value={footerConfig.icp_number || ''} onChange={e => setFooterConfig({ ...footerConfig, icp_number: e.target.value })} placeholder="京ICP备xxxxxxxx号" style={{ marginBottom: 8 }} />
        
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 6 }}>
          <Text style={labelStyle}>服务条款：名称 / 链接</Text>
          <Space size={6}>
            <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>前台展示开关</Text>
            <Switch
              size="small"
              checked={footerConfig.terms_enabled !== false}
              onChange={v => setFooterConfig({ ...footerConfig, terms_enabled: v })}
            />
          </Space>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8, marginBottom: 8 }}>
          <Input value={footerConfig.terms_text || ''} onChange={e => setFooterConfig({ ...footerConfig, terms_text: e.target.value })} placeholder="服务条款|Terms of Service" disabled={footerConfig.terms_enabled === false} />
          <Input value={footerConfig.terms_link || ''} onChange={e => setFooterConfig({ ...footerConfig, terms_link: e.target.value })} placeholder="/legal/terms" disabled={footerConfig.terms_enabled === false} />
        </div>

        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 6 }}>
          <Text style={labelStyle}>隐私政策：名称 / 链接</Text>
          <Space size={6}>
            <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>前台展示开关</Text>
            <Switch
              size="small"
              checked={footerConfig.privacy_enabled !== false}
              onChange={v => setFooterConfig({ ...footerConfig, privacy_enabled: v })}
            />
          </Space>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
          <Input value={footerConfig.privacy_text || ''} onChange={e => setFooterConfig({ ...footerConfig, privacy_text: e.target.value })} placeholder="隐私政策|Privacy Policy" disabled={footerConfig.privacy_enabled === false} />
          <Input value={footerConfig.privacy_link || ''} onChange={e => setFooterConfig({ ...footerConfig, privacy_link: e.target.value })} placeholder="/legal/privacy" disabled={footerConfig.privacy_enabled === false} />
        </div>
      </div>
    </div>
  );

  const handleCopyLink = async (path: string) => {
    const fullUrl = `${window.location.origin}${path}`;
    await copyWithFeedback(fullUrl, '链接已复制到剪贴板', '复制失败');
  };

  const renderStaticGen = () => (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0, color: _isLight ? '#1f2937' : '#fff' }}>静态 HTML 生成</Title>
      </div>
      <Alert type="info" showIcon message="生成后的静态 HTML 文件将部署到 /portal 路径，便于搜索引擎抓取和 SEO/GEO 优化" style={{ marginBottom: 16 }} />

      <div style={cardStyle}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 4 }}>手动静态 HTML 生成模式</Text>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12 }}>
              关闭：保存门户或风格后，自动重写 /portal 静态页。开启：保存后不自动重写，要点下面的按钮。用户正在访问的 /home 每次直接渲染，不看这个开关。
            </Text>
          </div>
          <Switch
            checked={staticGenConfig.manual_mode === true}
            onChange={async (checked) => {
              const prev = staticGenConfig;
              const next = { ...prev, manual_mode: checked };
              setStaticGenConfig(next);
              try {
                await request.post('/plugins/site-portal/portal-config', { section: 'static_gen', data: next });
                message.success(checked ? '已开启手动静态 HTML 生成模式' : '已关闭手动模式，保存后会自动更新静态页');
                fetchConfig({ silent: true });
              } catch (e) {
                console.error(e);
                setStaticGenConfig(prev);
                message.error('静态生成模式保存失败');
              }
            }}
          />
        </div>
      </div>

      {!staticGenConfig.manual_mode ? (
        <Alert 
          type="success" 
          showIcon 
          message="当前为「自动更新」：保存门户/风格配置后会后台自动生成 /portal 静态页，无需手动点击。" 
          style={{ marginBottom: 16 }} 
        />
      ) : (
        <Alert 
          type="warning" 
          showIcon 
          message="当前为「手动生成」：保存配置后不会自动更新，请点击下方按钮生成静态页。" 
          style={{ marginBottom: 16 }} 
        />
      )}

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 16 }}>快捷操作</Text>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 12 }}>
          <Button
            disabled={staticGenConfig.manual_mode !== true}
            type="primary"
            icon={<ThunderboltOutlined />}
            loading={generating}
            onClick={() => handleGenerate('all')}
          >
            全站生成
          </Button>
          <Button
            disabled={staticGenConfig.manual_mode !== true}
            loading={generating}
            onClick={() => handleGenerate('home')}
          >
            首页
          </Button>
          <Button
            disabled={staticGenConfig.manual_mode !== true}
            loading={generating}
            onClick={() => handleGenerate('columns', ['contact', 'about'])}
          >
            联系/关于
          </Button>
        </div>
      </div>

      {/* 生成后的快捷链接 */}
      {generatedLinks.length > 0 && (
        <div style={{
          ...cardStyle,
          background: _isLight ? 'linear-gradient(135deg, #f0fdf4, #dcfce7)' : 'linear-gradient(135deg, #052e16, #064e3b)',
          border: _isLight ? '1px solid #86efac' : '1px solid #065f46',
        }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            <LinkOutlined style={{ color: '#22c55e', fontSize: 16 }} />
            <Text strong style={{ color: _isLight ? '#166534' : '#86efac', fontSize: 14 }}>生成完成 - 快捷访问链接</Text>
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {generatedLinks.map((link, idx) => (
              <div key={idx} style={{
                display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                padding: '8px 12px', borderRadius: 6,
                background: _isLight ? 'rgba(255,255,255,0.7)' : 'rgba(0,0,0,0.2)',
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <Tag color="green" style={{ margin: 0 }}>{link.label}</Tag>
                  <a
                    href={link.path}
                    target="_blank"
                    rel="noopener noreferrer"
                    style={{ color: '#1677ff', fontSize: 13, textDecoration: 'none' }}
                  >
                    {window.location.origin}{link.path}
                  </a>
                </div>
                <Space size={4}>
                  <Button
                    type="text"
                    size="small"
                    icon={<CopyOutlined />}
                    onClick={() => handleCopyLink(link.path)}
                    style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}
                  />
                  <Button
                    type="link"
                    size="small"
                    icon={<EyeOutlined />}
                    onClick={() => window.open(link.path, '_blank')}
                  >
                    查看
                  </Button>
                </Space>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* 生成日志 */}
      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>最近生成记录</Text>
        {generateLog.length > 0 ? generateLog.slice(0, 10).map((log: any, idx: number) => (
          <div key={idx} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '8px 0', borderBottom: _isLight ? '1px solid rgba(0,0,0,0.04)' : '1px solid rgba(255,255,255,0.04)' }}>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, minWidth: 140 }}>{log.time}</Text>
            <Tag style={{ margin: 0 }}>{log.scope === 'all' ? '全站' : log.scope}</Tag>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13 }}>{(log.pages || []).join('、')}</Text>
          </div>
        )) : (
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)', fontSize: 13 }}>暂无生成记录</Text>
        )}
      </div>
    </div>
  );

  const renderOther = () => (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0, color: _isLight ? '#1f2937' : '#fff' }}>其他配置</Title>
        <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={(saveCooldowns['scripts'] || 0) > 0} onClick={handleSaveOther}>
          {(saveCooldowns['scripts'] || 0) > 0 ? `已保存 (${saveCooldowns['scripts']}s)` : '保存'}
        </Button>
      </div>
      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 4 }}>API Base URL / 基础 URL</Text>
        <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, display: 'block', marginBottom: 8 }}>
          统一用于首页顶部「API Base URL」与底部文案中的「基础 URL」。留空则使用当前站点 origin + /v1。
        </Text>
        <Input
          value={homeConfig.api_base_url || ''}
          onChange={e => setHomeConfig({ ...homeConfig, api_base_url: e.target.value })}
          placeholder="例如：https://api.example.com/v1"
        />
      </div>
      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 4 }}>客服代码</Text>
        <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, display: 'block', marginBottom: 8 }}>
          输入的 JS 代码将自动加载到所有门户页面（注入到 &lt;/body&gt; 前）
        </Text>
        <TextArea rows={6} value={customScripts.customer_service || ''} onChange={e => setCustomScripts({ ...customScripts, customer_service: e.target.value })}
          placeholder={'<script>\n// 客服系统 JS 代码\n</script>'} style={{ fontFamily: 'monospace', fontSize: 12 }} />
      </div>
      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 4 }}>统计代码</Text>
        <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, display: 'block', marginBottom: 8 }}>
          输入的 JS 代码将自动加载到所有门户页面（注入到 &lt;head&gt; 中）
        </Text>
        <TextArea rows={6} value={customScripts.analytics || ''} onChange={e => setCustomScripts({ ...customScripts, analytics: e.target.value })}
          placeholder={'<!-- Google Analytics -->\n<script async src="https://..."></script>'} style={{ fontFamily: 'monospace', fontSize: 12 }} />
      </div>
    </div>
  );

  const updateHomeFeature = (idx: number, patch: Record<string, string>) => {
    const features = [...(homeConfig.features || [])];
    features[idx] = { ...features[idx], ...patch };
    setHomeConfig({ ...homeConfig, features });
  };

  const renderHome = () => (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0, color: _isLight ? '#1f2937' : '#fff' }}>首页配置</Title>
        <Space>
          <Button icon={<EyeOutlined />} onClick={() => window.open('/home', '_blank')}>预览首页</Button>
          <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={(saveCooldowns['home'] || 0) > 0} onClick={() => handleSave('home', homeConfig)}>
            {(saveCooldowns['home'] || 0) > 0 ? `已保存 (${saveCooldowns['home']}s)` : '保存'}
          </Button>
        </Space>
      </div>

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>Hero 文案</Text>
        <Text style={labelStyle}>主标题</Text>
        <Input value={homeConfig.hero_title || ''} onChange={e => setHomeConfig({ ...homeConfig, hero_title: e.target.value })} placeholder="一个接口，调用全球数百个 AI 模型" style={{ marginBottom: 12 }} />
        <Text style={labelStyle}>副标题</Text>
        <TextArea rows={2} value={homeConfig.hero_subtitle || ''} onChange={e => setHomeConfig({ ...homeConfig, hero_subtitle: e.target.value })} placeholder="OpenAI兼容和原生格式，极速接入主流模型，零门槛开始" />
      </div>

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 4 }}>API Base URL / 基础 URL</Text>
        <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, display: 'block', marginBottom: 8 }}>
          用于首页顶部展示与底部 CTA 中的基础 URL。留空则使用当前站点 origin + /v1。
        </Text>
        <Input
          value={homeConfig.api_base_url || ''}
          onChange={e => setHomeConfig({ ...homeConfig, api_base_url: e.target.value })}
          placeholder="例如：https://api.example.com/v1"
        />
      </div>

      <div style={cardStyle}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
          <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>功能亮点</Text>
          <Button size="small" icon={<PlusOutlined />} onClick={() => {
            const features = [...(homeConfig.features || []), { icon: '', title: '', description: '' }];
            setHomeConfig({ ...homeConfig, features });
          }}>添加</Button>
        </div>
        {(homeConfig.features || []).map((feat: any, idx: number) => (
          <div key={idx} style={{ marginBottom: 12, paddingBottom: 12, borderBottom: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)' }}>
            <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
              <Input value={feat.title || ''} onChange={e => updateHomeFeature(idx, { title: e.target.value })} placeholder="标题" />
              <Button size="small" danger icon={<DeleteOutlined />} onClick={() => {
                const features = (homeConfig.features || []).filter((_: any, i: number) => i !== idx);
                setHomeConfig({ ...homeConfig, features });
              }} />
            </div>
            <TextArea rows={2} value={feat.description || ''} onChange={e => updateHomeFeature(idx, { description: e.target.value })} placeholder="描述" style={{ marginBottom: 8 }} />
            <Input value={feat.icon || ''} onChange={e => updateHomeFeature(idx, { icon: e.target.value })} placeholder="图标 SVG（可选）" />
          </div>
        ))}
      </div>

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>底部 CTA</Text>
        <Text style={labelStyle}>标题</Text>
        <Input value={homeConfig.cta_title || ''} onChange={e => setHomeConfig({ ...homeConfig, cta_title: e.target.value })} style={{ marginBottom: 12 }} />
        <Text style={labelStyle}>描述</Text>
        <TextArea rows={2} value={homeConfig.cta_description || ''} onChange={e => setHomeConfig({ ...homeConfig, cta_description: e.target.value })} style={{ marginBottom: 12 }} />
        <Text style={labelStyle}>主按钮文字 / 链接</Text>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8, marginBottom: 12 }}>
          <Input value={homeConfig.cta_primary_btn_text || ''} onChange={e => setHomeConfig({ ...homeConfig, cta_primary_btn_text: e.target.value })} placeholder="开始对话" />
          <Input value={homeConfig.cta_primary_btn_link || ''} onChange={e => setHomeConfig({ ...homeConfig, cta_primary_btn_link: e.target.value })} placeholder="https://..." />
        </div>
        <Text style={labelStyle}>次按钮文字 / 链接</Text>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
          <Input value={homeConfig.cta_secondary_btn_text || ''} onChange={e => setHomeConfig({ ...homeConfig, cta_secondary_btn_text: e.target.value })} placeholder="阅读文档" />
          <Input value={homeConfig.cta_secondary_btn_link || ''} onChange={e => setHomeConfig({ ...homeConfig, cta_secondary_btn_link: e.target.value })} placeholder="https://..." />
        </div>
      </div>
    </div>
  );

  const renderContact = () => (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0, color: _isLight ? '#1f2937' : '#fff' }}>联系我们</Title>
        <Space>
          <Button icon={<EyeOutlined />} onClick={() => window.open(portalPagePreviewPath(contactConfig.path, 'contact'), '_blank')}>预览页面</Button>
          <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={(saveCooldowns['contact'] || 0) > 0} onClick={() => handleSave('contact', contactSavePayload(contactConfig))}>
            {(saveCooldowns['contact'] || 0) > 0 ? `已保存 (${saveCooldowns['contact']}s)` : '保存'}
          </Button>
        </Space>
      </div>

      <div style={cardStyle}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12, paddingBottom: 10, borderBottom: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.08)' }}>
          <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>1. 页面基本属性</Text>
          <Switch checked={contactConfig.enabled !== false} onChange={v => setContactConfig({ ...contactConfig, enabled: v })} />
        </div>
        <Text style={labelStyle}>页面名称 (Title)</Text>
        <Input value={contactConfig.title || ''} onChange={e => setContactConfig({ ...contactConfig, title: e.target.value })} placeholder="例如：联系我们" style={{ marginBottom: 12 }} />
        <Text style={labelStyle}>路由路径 (Path Slug)</Text>
        <Input value={contactConfig.path || ''} onChange={e => setContactConfig({ ...contactConfig, path: e.target.value })} placeholder="例如：contact" />
      </div>

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 4, paddingBottom: 10, borderBottom: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.08)' }}>2. 官方联系渠道设置</Text>
        <Text type="secondary" style={{ fontSize: 12, display: 'block', marginBottom: 12 }}>排序数值越大越靠前</Text>
        {channelOrderFromSort(normalizeChannelSort(contactConfig.content)).map((key, idx, order) => {
          const field = CONTACT_CHANNEL_FIELDS.find((item) => item.key === key);
          if (!field) return null;
          const enabledKey = `${field.key}_enabled`;
          const channel_sort = normalizeChannelSort(contactConfig.content);
          return (
            <div key={field.key} style={{ marginBottom: idx === order.length - 1 ? 0 : 12 }}>
              <Text style={labelStyle}>{field.label}</Text>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <SortInput
                  value={channel_sort[field.key] ?? 0}
                  onChange={(sort) => {
                    const nextSort = { ...channel_sort, [field.key]: sort };
                    setContactConfig({
                      ...contactConfig,
                      content: { ...contactConfig.content, channel_sort: nextSort },
                    });
                  }}
                  onCommit={() => {
                    setContactConfig((prev: any) => {
                      const nextSort = normalizeChannelSort(prev.content);
                      return {
                        ...prev,
                        content: {
                          ...prev.content,
                          channel_sort: nextSort,
                          channel_order: channelOrderFromSort(nextSort),
                        },
                      };
                    });
                  }}
                />
                <Input
                  value={contactConfig.content?.[field.key] || ''}
                  onChange={e => setContactConfig({ ...contactConfig, content: { ...contactConfig.content, [field.key]: e.target.value } })}
                  placeholder={field.placeholder}
                  style={{ flex: 1, opacity: contactConfig.content?.[enabledKey] === false ? 0.45 : 1 }}
                />
                <Switch size="small" checked={contactConfig.content?.[enabledKey] !== false} onChange={v => setContactConfig({ ...contactConfig, content: { ...contactConfig.content, [enabledKey]: v } })} />
              </div>
            </div>
          );
        })}
      </div>

      <div style={cardStyle}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 14, paddingBottom: 10, borderBottom: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.08)' }}>
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, color: _isLight ? '#1f2937' : '#fff', fontSize: 14, fontWeight: 600 }}>
              <Share2 size={16} strokeWidth={1.5} /> 3. 社交媒体矩阵 (Social Links)
            </div>
            <Text type="secondary" style={{ fontSize: 12 }}>排序数值越大越靠前</Text>
          </div>
          <Button size="small" icon={<PlusOutlined />} onClick={() => {
            const social_links = [...(contactConfig.content?.social_links || []), { platform: '', value: '', enabled: true, sort: 0, key: `social_${Date.now()}` }];
            setContactConfig({ ...contactConfig, content: { ...contactConfig.content, social_links } });
          }}>添加社交平台</Button>
        </div>
        {!(contactConfig.content?.social_links || []).length ? (
          <div style={{
            textAlign: 'center',
            padding: '24px 0',
            color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)',
            fontSize: 13,
            background: _isLight ? 'rgba(0,0,0,0.03)' : 'rgba(255,255,255,0.04)',
            borderRadius: 6,
            border: _isLight ? '1px dashed rgba(0,0,0,0.12)' : '1px dashed rgba(255,255,255,0.12)',
          }}>
            暂无社交媒体链接，点击右上角添加按钮创建。
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {(contactConfig.content?.social_links || []).map((link: any, idx: number) => (
              <div key={link.key || idx} style={{ display: 'grid', gridTemplateColumns: '72px 44px minmax(140px, 1fr) minmax(200px, 1.6fr) 32px', gap: 12, alignItems: 'center', opacity: link.enabled === false ? 0.45 : 1 }}>
                <SortInput
                  value={parseSortValue(link.sort, 0)}
                  onChange={(sort) => {
                    const social_links = [...(contactConfig.content?.social_links || [])];
                    social_links[idx] = { ...link, sort };
                    setContactConfig({ ...contactConfig, content: { ...contactConfig.content, social_links } });
                  }}
                  onCommit={() => setContactConfig((prev: any) => ({
                    ...prev,
                    content: { ...prev.content, social_links: rankedList(prev.content?.social_links || []) },
                  }))}
                />
                <Switch
                  size="small"
                  checked={link.enabled !== false}
                  onChange={v => {
                    const social_links = [...(contactConfig.content?.social_links || [])];
                    social_links[idx] = { ...link, enabled: v };
                    setContactConfig({ ...contactConfig, content: { ...contactConfig.content, social_links } });
                  }}
                />
                <Input
                  value={link.platform || ''}
                  placeholder="平台名称 (如: Twitter / X)"
                  onChange={e => {
                    const social_links = [...(contactConfig.content?.social_links || [])];
                    social_links[idx] = { ...link, platform: e.target.value };
                    setContactConfig({ ...contactConfig, content: { ...contactConfig.content, social_links } });
                  }}
                />
                <Input
                  value={link.value || ''}
                  placeholder="跳转 URL 链接 (如: https://x.com/...)"
                  onChange={e => {
                    const social_links = [...(contactConfig.content?.social_links || [])];
                    social_links[idx] = { ...link, value: e.target.value };
                    setContactConfig({ ...contactConfig, content: { ...contactConfig.content, social_links } });
                  }}
                />
                <Button size="small" type="text" danger icon={<DeleteOutlined />} onClick={() => {
                  const social_links = (contactConfig.content?.social_links || []).filter((_: any, i: number) => i !== idx);
                  setContactConfig({ ...contactConfig, content: { ...contactConfig.content, social_links } });
                }} />
              </div>
            ))}
          </div>
        )}
      </div>

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 8, paddingBottom: 10, borderBottom: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.08)' }}>4. 尾部补充说明 (HTML 富文本 / Text)</Text>
        <TextArea
          rows={5}
          value={contactConfig.content?.custom_content || ''}
          onChange={e => setContactConfig({ ...contactConfig, content: { ...contactConfig.content, custom_content: e.target.value } })}
          placeholder="<p>补充说明...</p>"
          style={{ fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace', fontSize: 12 }}
        />
      </div>
    </div>
  );

  const renderAbout = () => (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0, color: _isLight ? '#1f2937' : '#fff' }}>关于我们</Title>
        <Space>
          <Button icon={<EyeOutlined />} onClick={() => window.open(portalPagePreviewPath(aboutConfig.path, 'about'), '_blank')}>预览页面</Button>
          <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={(saveCooldowns['about'] || 0) > 0} onClick={() => handleSave('about', aboutConfig)}>
            {(saveCooldowns['about'] || 0) > 0 ? `已保存 (${saveCooldowns['about']}s)` : '保存'}
          </Button>
        </Space>
      </div>

      <div style={cardStyle}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
          <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>启用关于我们页面</Text>
          <Switch checked={aboutConfig.enabled !== false} onChange={v => setAboutConfig({ ...aboutConfig, enabled: v })} />
        </div>
        <Text style={labelStyle}>页面标题</Text>
        <Input value={aboutConfig.title || ''} onChange={e => setAboutConfig({ ...aboutConfig, title: e.target.value })} placeholder="关于我们" style={{ marginBottom: 12 }} />
        <Text style={labelStyle}>路径 slug</Text>
        <Input value={aboutConfig.path || ''} onChange={e => setAboutConfig({ ...aboutConfig, path: e.target.value })} placeholder="about" />
      </div>

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 4 }}>Hero 副标题</Text>
        <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, display: 'block', marginBottom: 8 }}>与 FAI 艺术关于我们页同一份数据。多语言用 <Text code>中文|English|日本語|한국어|Tiếng Việt</Text>，只填中文时使用内置译文。</Text>
        <TextArea rows={3} value={aboutConfig.hero_subtitle || ''} onChange={e => setAboutConfig({ ...aboutConfig, hero_subtitle: e.target.value })} />
      </div>

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>核心数据</Text>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
          <div>
            <Text style={labelStyle}>月均请求 Token</Text>
            <Input value={aboutConfig.stat_tokens || ''} onChange={e => setAboutConfig({ ...aboutConfig, stat_tokens: e.target.value })} placeholder="10B+" />
          </div>
          <div>
            <Text style={labelStyle}>全球活跃开发者</Text>
            <Input value={aboutConfig.stat_developers || ''} onChange={e => setAboutConfig({ ...aboutConfig, stat_developers: e.target.value })} placeholder="10K+" />
          </div>
          <div>
            <Text style={labelStyle}>模型供应商</Text>
            <Input value={aboutConfig.stat_providers || ''} onChange={e => setAboutConfig({ ...aboutConfig, stat_providers: e.target.value })} placeholder="40+" />
          </div>
          <div>
            <Text style={labelStyle}>集成前沿模型</Text>
            <Input value={aboutConfig.stat_models || ''} onChange={e => setAboutConfig({ ...aboutConfig, stat_models: e.target.value })} placeholder="200+" />
          </div>
        </div>
      </div>

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 12 }}>合作卡片</Text>
        <Text style={labelStyle}>卡片标题</Text>
        <Input value={aboutConfig.press_title || ''} onChange={e => setAboutConfig({ ...aboutConfig, press_title: e.target.value })} style={{ marginBottom: 12 }} />
        <Text style={labelStyle}>卡片描述</Text>
        <TextArea rows={2} value={aboutConfig.press_desc || ''} onChange={e => setAboutConfig({ ...aboutConfig, press_desc: e.target.value })} style={{ marginBottom: 12 }} />
        <Text style={labelStyle}>跳转文案</Text>
        <Input value={aboutConfig.press_link_text || ''} onChange={e => setAboutConfig({ ...aboutConfig, press_link_text: e.target.value })} placeholder="联系我们洽谈合作 →" />
      </div>

      <div style={cardStyle}>
        <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block', marginBottom: 8 }}>底部扩展 HTML</Text>
        <TextArea
          rows={8}
          value={aboutConfig.content || ''}
          onChange={e => setAboutConfig({ ...aboutConfig, content: e.target.value })}
          placeholder="<p>补充关于我们的详细介绍...</p>"
          style={{ fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace', fontSize: 12 }}
        />
      </div>
    </div>
  );

  const renderCustomHomepage = () => (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0, color: _isLight ? '#1f2937' : '#fff' }}>自定义主页</Title>
        <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={(saveCooldowns['custom_homepage'] || 0) > 0} onClick={() => handleSave('custom_homepage', customHomepage)}>
          {(saveCooldowns['custom_homepage'] || 0) > 0 ? `已保存 (${saveCooldowns['custom_homepage']}s)` : '保存'}
        </Button>
      </div>

      <div style={cardStyle}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
          <div>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, display: 'block' }}>启用自定义主页</Text>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12 }}>
              关闭：使用托管首页（导航/底部生效），或在「风格选择」中开启风格化 Tera 首页。开启：直接渲染粘贴的 HTML（导航/底部/风格均不注入）。
            </Text>
          </div>
          <Switch
            checked={customHomepage.enabled}
            loading={saving}
            onChange={async v => {
              const newHtml = (v && !customHomepage.html) ? DEMO_HTML : customHomepage.html;
              const newCustomHomepage = { ...customHomepage, enabled: v, html: newHtml };
              setCustomHomepage(newCustomHomepage);
              if (v) {
                setActiveMenu('custom_homepage');
              }
              try {
                setSaving(true);
                await request.post('/plugins/site-portal/portal-config', { section: 'custom_homepage', data: newCustomHomepage });
                message.success(v ? '已开启自定义主页' : '已关闭自定义主页');
              } catch (e) {
                console.error(e);
              } finally {
                setSaving(false);
              }
            }}
          />
        </div>
      </div>

      {customHomepage.enabled && (
        <>
          <Alert
            message="自定义主页已启用"
            description="首页将直接显示粘贴的 HTML。导航/底部/风格选择不会注入到该 HTML；配置仍会保留，关闭开关即可恢复托管或风格化首页。"
            type="warning"
            showIcon
            style={{ marginBottom: 16 }}
          />

          <div style={cardStyle}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
              <div>
                <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>HTML 代码</Text>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, marginLeft: 8 }}>
                  粘贴完整的 HTML 页面代码（包含 &lt;html&gt;&lt;head&gt;&lt;body&gt; 等标签）
                </Text>
              </div>
              <Button size="small" onClick={() => {
                setCustomHomepage({ ...customHomepage, html: DEMO_HTML });
              }}>恢复默认演示模板</Button>
            </div>
            <TextArea
              rows={20}
              value={customHomepage.html || ''}
              onChange={e => setCustomHomepage({ ...customHomepage, html: e.target.value })}
              placeholder={'<!DOCTYPE html>\n<html lang="zh">\n<head>\n  <meta charset="UTF-8">\n  <meta name="viewport" content="width=device-width, initial-scale=1.0">\n  <title>我的自定义主页</title>\n  <style>\n    /* 您的样式 */\n  </style>\n</head>\n<body>\n  <!-- 您的内容 -->\n</body>\n</html>'}
              style={{
                fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
                fontSize: 12,
                lineHeight: '1.6',
                resize: 'vertical',
                minHeight: 400,
              }}
            />
            <div style={{ marginTop: 12, display: 'flex', alignItems: 'center', gap: 12 }}>
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
                💡 提示：您可以使用 AI 生成完整的单页 HTML 代码，然后粘贴到这里。保存后即可通过门户首页查看效果。
              </Text>
            </div>
          </div>
        </>
      )}
    </div>
  );

  if (loading) return <div style={{ textAlign: 'center', padding: 60 }}><Spin /></div>;

  const panels: Record<MenuKey, () => React.ReactNode> = {
    custom_homepage: renderCustomHomepage,
    home: renderHome,
    contact: renderContact,
    about: renderAbout,
    nav: renderNav,
    footer: renderFooter,
    static_gen: renderStaticGen,
    other: renderOther,
  };

  return (
    <div style={{ display: 'flex', gap: 16, minHeight: 500 }}>
      {/* Left Sidebar */}
      <div style={{
        width: 196, flexShrink: 0,
        background: _isLight ? '#fff' : '#141414',
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
        borderRadius: 8, padding: '8px 0', alignSelf: 'flex-start', position: 'sticky', top: 80,
      }}>
        {menuItems.map(item => {
          if (item.isTitle) {
            return (
              <div key={item.key} style={{
                display: 'flex', alignItems: 'center', gap: 8,
                padding: '10px 16px 4px 16px', fontSize: 12, fontWeight: 600,
                color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)',
              }}>
                {item.icon}
                {item.label}
              </div>
            );
          }
          return (
            <div
              key={item.key}
              onClick={() => setActiveMenu(item.key as MenuKey)}
              style={{
                display: 'flex', alignItems: 'center', gap: 8,
                padding: `10px 16px 10px ${item.isSub ? '32px' : '16px'}`, cursor: 'pointer', fontSize: 13, fontWeight: 500,
                color: activeMenu === item.key ? '#1677ff' : (_isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)'),
                background: activeMenu === item.key ? (_isLight ? 'rgba(22,119,255,0.06)' : 'rgba(22,119,255,0.08)') : 'transparent',
                borderRight: activeMenu === item.key ? '2px solid #1677ff' : '2px solid transparent',
                transition: 'all 0.15s',
              }}
            >
              {item.icon}
              {item.label}
            </div>
          );
        })}
      </div>

      {/* Right Panel */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <ErrorBoundary key={activeMenu}>
          {panels[activeMenu]?.()}
        </ErrorBoundary>
      </div>
    </div>
  );
};

export default PortalManager;

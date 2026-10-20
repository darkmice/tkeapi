/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect, useLayoutEffect, useRef, useCallback, useMemo } from 'react';
import { getAnnouncementLabel, getAnnouncementDisplayTime } from '../utils/announcement';
import {
  parseNotificationPreferences,
  shouldShowWebNotifications,
  maybeShowBrowserPush,
} from '../utils/notificationPrefs';
import { Sidebar as SidebarIcon, Terminal } from 'lucide-react';
import { fetchConsoleBootstrap } from '../utils/consoleBootstrap';
import { peekActivePlugins } from '../utils/activePlugins';
import {
  fetchAdminPlugins,
  peekAdminPlugins,
  subscribeAdminPluginsChange,
  selectAdminSidebarPlugins,
  adminSidebarPluginLabel,
  adminSidebarPluginHref,
} from '../utils/adminPlugins';
import { persistUserLanguagePreference, LANG_NAME_MAP, toDisplayLocale } from '../utils/language';
import useSettingsStore from '../store/settings';
import { hasAdminChildMenuPermission } from '../constants/adminMenuPermissions';
import ErrorBoundary from '../components/ErrorBoundary';
import { Layout, Menu, Button, Typography, ConfigProvider, theme, Grid } from 'antd';
import {
  DashboardOutlined,
  ControlOutlined,
  KeyOutlined,
  BarsOutlined,
  GiftOutlined,
  TeamOutlined,
  MenuUnfoldOutlined,
  MenuFoldOutlined,
  GlobalOutlined,
  LogoutOutlined,
  AppstoreOutlined,
  SettingOutlined,
  WalletOutlined,
  UserOutlined,
  NotificationOutlined,
  HistoryOutlined,
  ScheduleOutlined,
  RocketOutlined,
  PictureOutlined,
  FolderOpenOutlined,
  ExperimentOutlined,
  InfoCircleOutlined,
  BellOutlined,
  ShopOutlined,
  SunOutlined,
  MoonOutlined,

  VideoCameraOutlined,
  SafetyCertificateOutlined,
  RightOutlined,
  SoundOutlined,
  FileTextOutlined,
  ShoppingOutlined,
} from '@ant-design/icons';

import { Link, useLocation, useNavigate, Outlet } from 'react-router-dom';
import request from '../utils/request';
import { couponCampaignPath, offerReturnState } from '../pages/Credits/storefront/path';
import { useTranslation } from 'react-i18next';
import { Dropdown, Modal, message, Popover, Avatar, Divider, Drawer, List, Badge, Tooltip, Checkbox } from 'antd';
import type { MenuProps } from 'antd';
import type { Announcement, Plugin } from '../types';
import useAuthStore from '../store/auth';
import { normalizeTitleHref } from './AuthLayout';
import { useThemeStore } from '../store/theme';
import UserAvatarMenu from '../components/UserAvatarMenu';
import BindPromptModal from '../components/BindPromptModal';
import AnnouncementPopupModal from '../components/AnnouncementPopupModal';
import { getAntdThemeTokens, getSiderMenuTokens, softAccent } from '../theme/tokens';

const { Header, Sider, Content } = Layout;
const { Title } = Typography;
const { useBreakpoint } = Grid;

interface DashboardLayoutProps {
  isUserEnd?: boolean;
}

/** 只比较会改变侧栏可见性的字段，避免 bootstrap 回包后整表重绘 */
function userPluginMenuSignature(list: any[]): string {
  return list
    .map((p) => `${p?.name}\0${p?.allowed_levels}\0${p?.user_asset_menu_enabled ? 1 : 0}`)
    .sort()
    .join('\n');
}

function adminPluginMenuSignature(list: Plugin[]): string {
  return list
    .map((p) => `${p.name}\0${p.is_enabled}\0${p.show_in_admin_menu ?? ''}\0${p.admin_menu_sort ?? ''}\0${p.admin_menu_title ?? ''}\0${p.admin_menu_default_tab ?? ''}`)
    .sort()
    .join('\n');
}

/** 用户端个人中心/用户设置「我的钱包」定制菜单图标 */
const WalletMenuIcon: React.FC<{ style?: React.CSSProperties }> = ({ style }) => (
  <span
    role="img"
    aria-label="wallet"
    className="anticon anticon-wallet"
    style={{ fontSize: '18px', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', ...style }}
  >
    <svg width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M5 7.5V5.2A1.7 1.7 0 0 1 6.7 3.5h10.6A1.7 1.7 0 0 1 19 5.2v2.3" strokeWidth="1.6" />
      <rect x="2.5" y="7.5" width="19" height="13" rx="2.5" />
      <path d="M15 11.2h4.5a2 2 0 0 1 2 2v1.6a2 2 0 0 1-2 2H15a1.2 1.2 0 0 1-1.2-1.2v-3.2a1.2 1.2 0 0 1 1.2-1.2z" fill="currentColor" fillOpacity="0.16" />
      <circle cx="17.6" cy="14" r="1.15" fill="currentColor" />
    </svg>
  </span>
);

/** 用户端个人中心/用户设置「卡券订阅」定制菜单图标 */
const CouponSubscriptionMenuIcon: React.FC<{ style?: React.CSSProperties }> = ({ style }) => (
  <span
    role="img"
    aria-label="coupon-subscription"
    className="anticon anticon-coupon-subscription"
    style={{ fontSize: '18px', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', ...style }}
  >
    <svg width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2.5 8A2.5 2.5 0 0 1 5 5.5h2.5a1.5 1.5 0 0 0 3 0h8.5A2.5 2.5 0 0 1 21.5 8v8A2.5 2.5 0 0 1 19 18.5h-8.5a1.5 1.5 0 0 0-3 0H5A2.5 2.5 0 0 1 2.5 16Z" />
      <path d="M9 7.5v9" strokeDasharray="1.8 1.8" strokeWidth="1.5" />
      <path d="M6.8 10l-1.8 4M4.8 10.2h.01M6.8 13.8h.01" strokeWidth="1.5" />
      <path d="M15.5 8.5c0 1.9-1.3 3.5-3.5 3.5 2.2 0 3.5 1.6 3.5 3.5 0-1.9 1.3-3.5 3.5-3.5-2.2 0-3.5-1.6-3.5-3.5z" fill="currentColor" fillOpacity="0.2" strokeWidth="1.2" />
    </svg>
  </span>
);

const DashboardLayout: React.FC<DashboardLayoutProps> = ({ isUserEnd = false }) => {
  const { t, i18n } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();
  const screens = useBreakpoint();
  const [collapsed, setCollapsed] = useState(() => {
    try {
      if (typeof window !== 'undefined' && window.innerWidth <= 576) {
        return true;
      }
      const saved = localStorage.getItem('sidebar_collapsed');
      return saved !== null ? JSON.parse(saved) : false;
    } catch (e) {
      return false;
    }
  });

  useEffect(() => {
    if (screens.xs) {
      setCollapsed(true);
    }
  }, [screens.xs]);

  const handleCollapsedChange = (val: boolean) => {
    setCollapsed(val);
    localStorage.setItem('sidebar_collapsed', JSON.stringify(val));
  };

  const [openKeys, setOpenKeys] = useState<string[]>([]);


  const { user, logout, setUser, isLoggedIn } = useAuthStore();
  const { themeMode, toggleTheme } = useThemeStore();
  // 复用 App.tsx 中已拉取的 settings store，不再独立调 /settings 接口
  const { settings } = useSettingsStore();
  const adminPath = settings?.site?.admin_path || localStorage.getItem('tokensbyte_admin_path') || 'admin1688';
  const site = settings?.site;
  const siteName = site?.name || 'Tkeapi';
  const siteLogo = site?.logo || '';
  const logoTitleHref = normalizeTitleHref(site?.logo_title_url);
  const goLogoTitle = () => {
    if (!logoTitleHref) return;
    if (logoTitleHref.startsWith('/') || logoTitleHref.startsWith('#') || logoTitleHref.startsWith('?')) {
      navigate(logoTitleHref);
    } else {
      window.location.href = logoTitleHref;
    }
  };
  const siteTitle = site?.title || '';
  const enableMultilingual = site?.enable_multilingual !== false;
  const enableThemeToggle = site?.enable_theme_toggle !== false;
  const supportedLanguages = site?.supported_languages?.length ? site.supported_languages : ['zh', 'en'];
  const agreement = settings?.agreement || null;

  const [announcementsDrawerVisible, setAnnouncementsDrawerVisible] = useState(false);
  const [announcements, setAnnouncements] = useState<Announcement[]>([]);
  const [popupAnnouncements, setPopupAnnouncements] = useState<Announcement[]>([]);
  const [popupModalOpen, setPopupModalOpen] = useState(false);
  const [consolePageReady, setConsolePageReady] = useState(false);
  const dashboardPopupShownRef = useRef(false);
  const notifyConsolePageReady = useCallback(() => {
    setTimeout(() => {
      setConsolePageReady(true);
    }, 2000);
  }, []);
  const outletContextValue = useMemo(() => ({
    announcements,
    notifyConsolePageReady,
  }), [announcements, notifyConsolePageReady]);
  const [unreadCount, setUnreadCount] = useState(0);
  const [ticketUnread, setTicketUnread] = useState(0);
  const [activePlugins, setActivePlugins] = useState<any[]>(() => peekActivePlugins() ?? []);
  const [userPluginsReady, setUserPluginsReady] = useState(
    () => !isUserEnd || peekActivePlugins() !== null,
  );
  const [adminPlugins, setAdminPlugins] = useState<Plugin[]>(() => peekAdminPlugins() ?? []);
  const [adminPluginsReady, setAdminPluginsReady] = useState(
    () => isUserEnd || import.meta.env.VITE_ENABLE_PLUGINS !== 'true' || peekAdminPlugins() !== null,
  );
  const [bootstrapDone, setBootstrapDone] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    contentRef.current?.scrollTo(0, 0);
  }, [location.pathname, location.search]);

  useEffect(() => {
    if (!isLoggedIn) return;
    let cancelled = false;
    fetchConsoleBootstrap()
      .then((data) => {
        if (cancelled) return;
        if (data.user?.id) {
          setUser(data.user, !!sessionStorage.getItem('token'));
        }
        if (Array.isArray(data.announcements)) {
          setAnnouncements(data.announcements);
        }
        if (Array.isArray(data.active_plugins)) {
          const next = data.active_plugins;
          setActivePlugins((prev) =>
            userPluginMenuSignature(prev) === userPluginMenuSignature(next) ? prev : next,
          );
        }
      })
      .catch((error) => {
        console.error('Failed to fetch console bootstrap', error);
      })
      .finally(() => {
        if (!cancelled) {
          setUserPluginsReady(true);
          setBootstrapDone(true);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [isLoggedIn]);

  useEffect(() => {
    if (isUserEnd || !isLoggedIn) return;
    if (import.meta.env.VITE_ENABLE_PLUGINS !== 'true') return;
    let cancelled = false;
    const load = () => {
      fetchAdminPlugins()
        .then((data) => {
          if (cancelled) return;
          const next = Array.isArray(data?.plugins) ? data.plugins : [];
          setAdminPlugins((prev) =>
            adminPluginMenuSignature(prev) === adminPluginMenuSignature(next) ? prev : next,
          );
        })
        .catch((error) => {
          console.error('Failed to fetch admin plugins', error);
        })
        .finally(() => {
          if (!cancelled) setAdminPluginsReady(true);
        });
    };
    load();
    const unsub = subscribeAdminPluginsChange(load);
    return () => {
      cancelled = true;
      unsub();
    };
  }, [isLoggedIn, isUserEnd]);


  const showSystemAbout = () => {
    navigate(`/${adminPath}/about`);
  };

  const changeLanguage = (lng: string) => {
    i18n.changeLanguage(lng);
    persistUserLanguagePreference(lng);
  };

  const implementedLangs = i18n.options.resources ? Object.keys(i18n.options.resources) : ['zh', 'en'];

  const langItems: MenuProps['items'] = supportedLanguages
    .filter(lng => implementedLangs.includes(lng))
    .map(lng => ({
      key: lng,
      label: LANG_NAME_MAP[lng] || lng,
      onClick: () => changeLanguage(lng),
    }));

  const isDashboardPath = (pathname: string) => {
    const p = pathname.endsWith('/') && pathname.length > 1 ? pathname.slice(0, -1) : pathname;
    return p === '/' || p === '/dashboard' || p.endsWith('/dashboard');
  };

  const checkAndShowPopup = (dataList: Announcement[]) => {
    const now = new Date();
    const todayStr = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    const targetPopups = dataList.filter((item: Announcement) => {
      if (item.is_popup !== 1) return false;
      return localStorage.getItem(`popup_notice_dismissed_${item.id}`) !== todayStr;
    });
    if (targetPopups.length === 0) return false;
    setPopupAnnouncements(targetPopups);
    setPopupModalOpen(true);
    return true;
  };

  // 当公告数据、用户偏好或阈值设置更新时，计算未读数与浏览器推送，不再发起多余网络请求
  useEffect(() => {
    if (!announcements.length) {
      setUnreadCount(0);
      return;
    }
    const prefs = parseNotificationPreferences(
      user?.notification_preferences,
      settings?.notification?.low_balance_threshold ?? 100.0,
    );
    const showWeb = shouldShowWebNotifications(prefs, settings?.notification);
    setUnreadCount(showWeb ? announcements.length : 0);
    if (showWeb && announcements.length > 0) {
      const first = announcements[0];
      const seenKey = `notif_push_seen_${first.id}`;
      if (!sessionStorage.getItem(seenKey)) {
        sessionStorage.setItem(seenKey, '1');
        const title = getAnnouncementLabel(first.title || '') || t('header.notifications', '通知');
        const body = getAnnouncementLabel(first.content || '').replace(/<[^>]+>/g, '').slice(0, 120);
        maybeShowBrowserPush(title, body, prefs, settings?.notification);
      }
    }
  }, [announcements, user?.notification_preferences, settings?.notification?.low_balance_threshold, settings?.notification, i18n.language]);

  // 离开控制台时关掉弹窗，下次进入再等加载完成后弹
  useEffect(() => {
    if (!isDashboardPath(location.pathname)) {
      dashboardPopupShownRef.current = false;
      setConsolePageReady(false);
      setPopupModalOpen(false);
    }
  }, [location.pathname]);

  // 控制台首屏加载完成后再弹一次，避免加载过程中反复挂载闪屏
  useEffect(() => {
    if (!isDashboardPath(location.pathname) || !consolePageReady) return;
    if (dashboardPopupShownRef.current || announcements.length === 0) return;
    if (checkAndShowPopup(announcements)) {
      dashboardPopupShownRef.current = true;
    }
  }, [location.pathname, announcements, consolePageReady]);

  const handleClosePopupModal = (dontShowToday: boolean, noticeIds: number[]) => {
    if (dontShowToday) {
      const now = new Date();
      const todayStr = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
      noticeIds.forEach((id) => {
        localStorage.setItem(`popup_notice_dismissed_${id}`, todayStr);
      });
    }
    setPopupModalOpen(false);
  };

  // 插件菜单：检查用户等级是否在插件允许范围内
  const isPluginVisibleForUser = (pluginName: string) => {
    const plugin = activePlugins.find((p: any) => p.name === pluginName);
    if (!plugin) return false;
    if (plugin.allowed_levels === 'all') return true;
    if (!isUserEnd) return true; // 管理员端始终显示
    const allowed = plugin.allowed_levels.split(',');
    const userGroup = user?.user_group || '';
    const levelId = user?.level_id != null ? String(user.level_id) : '';
    return allowed.includes(userGroup) || (levelId !== '' && allowed.includes(levelId));
  };

  const [couponCampaign, setCouponCampaign] = useState<{ id: number; template_key?: string } | null>(null);
  useEffect(() => {
    if (!isUserEnd || !isLoggedIn || !isPluginVisibleForUser('marketing_membership')) {
      setCouponCampaign(null);
      return;
    }
    let dead = false;
    request
      .get('/plugins/marketing_membership/placements/console_nav_coupon', { skipErrorHandler: true } as any)
      .then((res: any) => {
        if (dead) return;
        const listed = Array.isArray(res?.campaigns) ? res.campaigns : [];
        const first = listed.find((row: any) => row?.id && (row?.plans || []).length > 0);
        setCouponCampaign(first?.id ? { id: first.id, template_key: first.template_key } : null);
      })
      .catch(() => {
        if (!dead) setCouponCampaign(null);
      });
    return () => {
      dead = true;
    };
  }, [isUserEnd, isLoggedIn, activePlugins, user?.user_group, user?.level_id]);
  const showTicketButton = isUserEnd && isLoggedIn && isPluginVisibleForUser('support_tickets');
  useEffect(() => {
    if (!showTicketButton) {
      setTicketUnread(0);
      return undefined;
    }
    let dead = false;
    const load = () => {
      request
        .get('/plugins/support_tickets/unread', { skipErrorHandler: true } as any)
        .then((res: any) => {
          if (!dead) setTicketUnread(Number(res?.count) || 0);
        })
        .catch(() => {});
    };
    load();
    const timer = window.setInterval(load, 30000);
    const onFocus = () => load();
    const onRead = () => load();
    window.addEventListener('focus', onFocus);
    window.addEventListener('support-tickets-read', onRead);
    return () => {
      dead = true;
      window.clearInterval(timer);
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('support-tickets-read', onRead);
    };
  }, [showTicketButton, location.pathname]);

  const menuItems: MenuProps['items'] = [];
  const isSuperAdmin = !isUserEnd && user?.role === 'admin' && !user.admin_group_id;

  const getMenuLabel = (item: any) => {
    if (!item) return '';
    const lang = (i18n.language || 'zh').toLowerCase();
    const isSimplifiedZh = lang === 'zh' || lang === 'zh-cn' || lang === 'zh-hans' || lang === 'zh-sg';
    if (isSimplifiedZh) {
      if (item.label_zh && item.label_zh.trim()) {
        return item.label_zh;
      }
      if (item.label_en && item.label_en.trim()) {
        return item.label_en;
      }
    } else if (lang.startsWith('en')) {
      if (item.label_en && item.label_en.trim()) {
        return item.label_en;
      }
      if (item.label_zh && item.label_zh.trim()) {
        return item.label_zh;
      }
    } else {
      // 其它语种（如 zh-TW, ja, ko, vi 等）优先尝试国际化翻译字典，未匹配再降级到中文/英文
      if (item.key === '/playground' && i18n.exists('playground:title')) {
        return t('playground:title');
      }
      if (item.key === '/playground-2026' && i18n.exists('playground_2026:title')) {
        return t('playground_2026:title');
      }
      const i18nKey = `menu.${item.key.substring(1).replace(/-/g, '_')}`;
      if (i18n.exists(i18nKey)) {
        return t(i18nKey);
      }
      if (lang.startsWith('zh')) {
        return item.label_zh || item.label_en || item.key;
      }
      return item.label_en || item.label_zh || item.key;
    }

    if (item.key === '/playground' && i18n.exists('playground:title')) {
      return t('playground:title');
    }
    if (item.key === '/playground-2026' && i18n.exists('playground_2026:title')) {
      return t('playground_2026:title');
    }
    const i18nKey = `menu.${item.key.substring(1).replace(/-/g, '_')}`;
    if (i18n.exists(i18nKey)) {
      return t(i18nKey);
    }
    return item.label_zh || item.label_en || item.key;
  };

  const getMenuIcon = (iconName: string) => {
    const iconStyle = { fontSize: '18px' };
    switch (iconName) {
      case 'DashboardOutlined': return <DashboardOutlined style={iconStyle} />;
      case 'ExperimentOutlined': return <ExperimentOutlined style={iconStyle} />;
      case 'RocketOutlined': return <RocketOutlined style={iconStyle} />;
      case 'KeyOutlined': return <KeyOutlined style={iconStyle} />;
      case 'HistoryOutlined': return <HistoryOutlined style={iconStyle} />;
      case 'ScheduleOutlined': return <ScheduleOutlined style={iconStyle} />;
      case 'PictureOutlined': return <PictureOutlined style={iconStyle} />;
      case 'FolderOpenOutlined': return <FolderOpenOutlined style={iconStyle} />;
      case 'TeamOutlined': return <TeamOutlined style={iconStyle} />;

      case 'WalletOutlined': return <WalletOutlined style={iconStyle} />;
      case 'VideoCameraOutlined': return <VideoCameraOutlined style={iconStyle} />;
      case 'SafetyCertificateOutlined': return <SafetyCertificateOutlined style={iconStyle} />;
      case 'UserOutlined': return <UserOutlined style={iconStyle} />;
      case 'SettingOutlined': return <SettingOutlined style={iconStyle} />;
      default: return <BarsOutlined style={iconStyle} />;
    }
  };

  const isMenuAllowedForUser = (item: any) => {
    if (!item.enabled) return false;
    if (item.allowed_levels === 'all') return true;
    
    const allowed = item.allowed_levels.split(',');
    const userGroup = user?.user_group || '';
    const levelId = user?.level_id != null ? String(user.level_id) : '';
    return allowed.includes(userGroup) || (levelId !== '' && allowed.includes(levelId));
  };

  if (isUserEnd) {
    if (settings) {
      const defaultItems = [
        { key: '/dashboard', label_zh: '系统概览', label_en: 'Dashboard', icon: 'DashboardOutlined', enabled: true, sort_order: 1, allowed_levels: 'all' },
        { key: '/playground', label_zh: '创作中心', label_en: 'Playground', icon: 'ExperimentOutlined', enabled: true, sort_order: 2, allowed_levels: 'all' },
        { key: '/playground-2026', label_zh: '创作中心2026', label_en: 'Playground 2026', icon: 'ExperimentOutlined', enabled: true, sort_order: 2.5, allowed_levels: 'all' },
        { key: '/tokens', label_zh: '令牌管理', label_en: 'Tokens', icon: 'KeyOutlined', enabled: true, sort_order: 4, allowed_levels: 'all' },
        { key: '/logs', label_zh: '日志记录', label_en: 'Logs', icon: 'HistoryOutlined', enabled: true, sort_order: 5, allowed_levels: 'all' },
        { key: '/task-logs', label_zh: '任务列表', label_en: 'Task Logs', icon: 'ScheduleOutlined', enabled: true, sort_order: 6, allowed_levels: 'all' },
        { key: '/advanced-marketing', label_zh: '高级推广', label_en: 'Advanced Marketing', icon: 'TeamOutlined', enabled: true, sort_order: 10, allowed_levels: 'all' },

        { key: '/wallet', label_zh: '我的钱包', label_en: 'Wallet', icon: 'WalletOutlined', enabled: true, sort_order: 11, allowed_levels: 'all' },
        { key: '/ark-video-monitor', label_zh: '视频监控', label_en: 'Ark Video Monitor', icon: 'VideoCameraOutlined', enabled: true, sort_order: 11.5, allowed_levels: 'all' },
        { key: '/profile', label_zh: '个人中心', label_en: 'Profile', icon: 'UserOutlined', enabled: true, sort_order: 12, allowed_levels: 'all' },
      ];

      const mergedItems = settings?.menu_config?.items?.length ? [...settings.menu_config.items] : [...defaultItems];

      defaultItems.forEach((defItem) => {
        if (!mergedItems.some((item: any) => item.key === defItem.key)) {
          mergedItems.push({
            ...defItem,
            sort_order: mergedItems.length + 1
          });
        }
      });

      const sortedConfigs = mergedItems.sort((a: any, b: any) => (a.sort_order || 0) - (b.sort_order || 0));

      const userSettingsChildren: any[] = [];
      let settingsSortOrder = 11;

      sortedConfigs.forEach((item: any) => {
        if (!isMenuAllowedForUser(item)) return;
        if (item.key === '/relay-api' || item.key === '/docs') return; // Completely hide API tutorial from left menu

        if (item.key === '/moderation-query' || item.key === '/assets' || item.key === '/assets-intl') return;
        if (item.key === '/playground' && !isPluginVisibleForUser('playground')) return;
        if (item.key === '/playground-2026' && !isPluginVisibleForUser('playground_2026')) return;
        if (item.key === '/advanced-marketing' && !isPluginVisibleForUser('team_marketing')) return;

        if (item.key === '/ark-video-monitor' && !isPluginVisibleForUser('volcengine_ark_monitor')) return;

        const labelNode = <Link to={item.key}>{getMenuLabel(item)}</Link>;

        if (item.key === '/wallet' || item.key === '/profile') {
          userSettingsChildren.push({
            key: item.key,
            icon: item.key === '/wallet' ? <WalletMenuIcon /> : getMenuIcon(item.icon),
            label: labelNode,
          });
          if (item.key === '/wallet') {
            settingsSortOrder = item.sort_order || 11;
          }
          return;
        }

        menuItems.push({
          key: item.key,
          icon: getMenuIcon(item.icon),
          label: labelNode,
          sort_order: item.sort_order,
        } as any);
      });

        if (settings?.notification?.site_notification_enabled) {
          userSettingsChildren.push({
            key: '/profile/notifications',
            icon: <BellOutlined style={{ fontSize: '18px' }} />,
            label: <Link to="/profile/notifications">{t('menu.notifications')}</Link>,
          });
        }

        userSettingsChildren.push({
          key: '/orders',
          icon: <ShoppingOutlined style={{ fontSize: '18px' }} />,
          label: <Link to="/orders">{t('menu.my_orders', '订单管理')}</Link>,
        });
        const membershipEnabled = activePlugins.some((p: any) => p.name === 'marketing_membership');
        const redemptionEnabled = !!settings?.marketing?.enable_redemption;
        if (membershipEnabled || redemptionEnabled) {
          userSettingsChildren.push({
            key: '/credits/records',
            icon: <CouponSubscriptionMenuIcon />,
            label: <Link to="/credits/records">{t('menu.coupon_records', '卡券订阅')}</Link>,
          });
        }

        const isInvoiceEnabled = user?.invoice_enabled === 1 || user?.role === 'admin';
        if (isInvoiceEnabled) {
          userSettingsChildren.push({
            key: '/invoices',
            icon: <FileTextOutlined style={{ fontSize: '18px' }} />,
            label: <Link to="/invoices">{t('menu.invoices', '财务发票')}</Link>,
          });
        }

        userSettingsChildren.sort((a, b) => {
          const rank = (key: string) => {
            if (key === '/profile') return 0;
            if (key === '/wallet') return 1;
            if (key === '/credits/records') return 3;
            if (key === '/orders') return 4;
            if (key === '/invoices') return 5;
            return 6;
          };
          return rank(a.key) - rank(b.key);
        });
        menuItems.push({
          key: 'user-settings-group',
          icon: getMenuIcon('SettingOutlined'),
          label: t('menu.user_settings'),
          children: userSettingsChildren,
          sort_order: settingsSortOrder,
        } as any);
        menuItems.sort((a: any, b: any) => (a.sort_order || 0) - (b.sort_order || 0));
    }
  } else {
    // Admin side menu construction
    const isSubAdmin = user?.role === 'admin' && !isSuperAdmin;
    const permissionsLoaded = !!user?.permissions;

    // Only render restricted core menus once permissions are loaded (or if super admin) to prevent flashing
    if (!isSubAdmin || permissionsLoaded) {
      if (!isSubAdmin || user?.permissions?.includes('dashboard')) {
        menuItems.push({
          key: '/admin0755/dashboard',
          icon: <DashboardOutlined style={{ fontSize: '18px' }} />,
          label: <Link to="/admin0755/dashboard">{t('menu.dashboard')}</Link>,
        });
      }

      // Hide API tutorial from left menu in Admin section as well (since it's moved to header)
      /* 
      if (!isSubAdmin || user?.permissions?.includes('relay_api')) {
        menuItems.push({
          key: '/admin0755/docs',
          icon: <RocketOutlined style={{ fontSize: '18px' }} />,
          label: <Link to="/admin0755/docs">{t('menu.relay_api')}</Link>,
        });
      }
      */

      if (!isSubAdmin || user?.permissions?.includes('tokens')) {
        menuItems.push({
          key: '/admin0755/tokens',
          icon: <KeyOutlined style={{ fontSize: '18px' }} />,
          label: <Link to="/admin0755/tokens">{t('menu.tokens')}</Link>,
        });
      }

      if (!isSubAdmin || user?.permissions?.includes('logs')) {
        if (
          !isSubAdmin ||
          hasAdminChildMenuPermission(user?.permissions, 'logs', 'logs.usage', false)
        ) {
          menuItems.push({
            key: '/admin0755/logs',
            icon: <HistoryOutlined style={{ fontSize: '18px' }} />,
            label: <Link to="/admin0755/logs">{t('menu.usage_logs')}</Link>,
          });
        }

        if (
          !isSubAdmin ||
          hasAdminChildMenuPermission(user?.permissions, 'logs', 'logs.tasks', false)
        ) {
          menuItems.push({
            key: '/admin0755/task-logs',
            icon: <ScheduleOutlined style={{ fontSize: '18px' }} />,
            label: <Link to="/admin0755/task-logs">{t('menu.task_logs')}</Link>,
          });
        }
      }
    }
  }

  if (!isUserEnd && user?.role === 'admin') {
    const hasPermission = (key: string) => {
      if (isSuperAdmin) return true; // 超级管理员直接放行所有菜单
      if (!user.permissions) return false;
      return user.permissions.includes(key);
    };
    const hasChildMenu = (parentKey: string, childKey: string) =>
      hasAdminChildMenuPermission(user?.permissions, parentKey, childKey, isSuperAdmin);

    if (hasPermission('dashboard')) {
      // dashboard is already at index 0, but we might want to consolidate menu items here
    }

    if (hasPermission('channels')) {
      const channelChildren = [];
      if (hasChildMenu('channels', 'channels.groups')) {
        channelChildren.push({
          key: '/admin0755/channels',
          label: <Link to="/admin0755/channels">{t('menu.channel_groups')}</Link>,
        });
      }
      if (hasChildMenu('channels', 'channels.configs')) {
        channelChildren.push({
          key: '/admin0755/channel-configs',
          label: <Link to="/admin0755/channel-configs">{t('menu.channel_configs')}</Link>,
        });
      }
      if (channelChildren.length > 0) {
        menuItems.push({
          key: 'channels-management-group',
          icon: <ControlOutlined style={{ fontSize: '18px' }} />,
          label: t('menu.channels'),
          children: channelChildren
        });
      }
    }

    if (hasPermission('models') || isSuperAdmin) {
      const modelChildren = [];

      if (hasChildMenu('models', 'models.list')) {
        modelChildren.push({
          key: '/admin0755/models',
          label: <Link to="/admin0755/models">{t('menu.model_list')}</Link>,
        });
      }
      if (hasChildMenu('models', 'models.billing_rules')) {
        modelChildren.push({
          key: '/admin0755/billing-rules',
          label: <Link to="/admin0755/billing-rules">{t('menu.billing_rules')}</Link>,
        });
      }
      if (hasChildMenu('models', 'models.forward_rules')) {
        modelChildren.push({
          key: '/admin0755/forward-rules',
          label: <Link to="/admin0755/forward-rules">{t('menu.forward_rules')}</Link>,
        });
      }

      if (modelChildren.length > 0) {
        menuItems.push({
          key: 'models-management-group',
          icon: <AppstoreOutlined style={{ fontSize: '18px' }} />,
          label: t('menu.admin_routing'),
          children: modelChildren
        });
      }
    }

    if (hasPermission('marketing')) {
      const marketingChildren = [];
      if (hasChildMenu('marketing', 'marketing.redemptions')) {
        marketingChildren.push({
          key: '/admin0755/redemptions',
          label: <Link to="/admin0755/redemptions">{t('menu.redemptions')}</Link>,
        });
      }
      if (hasChildMenu('marketing', 'marketing.registration_gifts')) {
        marketingChildren.push({
          key: '/admin0755/marketing/registration-gifts',
          label: <Link to="/admin0755/marketing/registration-gifts">{t('menu.registration_gifts')}</Link>,
        });
      }
      if (hasChildMenu('marketing', 'marketing.announcements')) {
        marketingChildren.push({
          key: '/admin0755/marketing/announcements',
          label: <Link to="/admin0755/marketing/announcements">{t('menu.announcements')}</Link>,
        });
      }
      if (marketingChildren.length > 0) {
        menuItems.push({
          key: 'marketing-management-group',
          icon: <NotificationOutlined style={{ fontSize: '18px' }} />,
          label: t('menu.marketing'),
          children: marketingChildren
        });
      }
    }


    if (hasPermission('users')) {
      const userItems = [];
      if (hasChildMenu('users', 'users.list')) {
        userItems.push({
          key: '/admin0755/users',
          label: <Link to="/admin0755/users">{t('menu.user_list')}</Link>,
        });
      }
      if (hasChildMenu('users', 'users.admins')) {
        userItems.push({
          key: '/admin0755/admins',
          label: <Link to="/admin0755/admins">{t('menu.admin_list')}</Link>,
        });
      }
      const canUserLevels = hasChildMenu('users', 'users.levels');
      const canAdminGroups = hasChildMenu('users', 'admin_groups');
      if (canUserLevels || canAdminGroups) {
        userItems.push({
          key: '/admin0755/user-levels',
          label: (
            <Link to={canUserLevels ? '/admin0755/user-levels' : '/admin0755/user-levels?tab=admins'}>
              {t('menu.user_levels')}
            </Link>
          ),
        });
      }
      if (hasChildMenu('users', 'users.analytics')) {
        userItems.push({
          key: '/admin0755/users/analytics',
          label: <Link to="/admin0755/users/analytics">{t('menu.user_analytics')}</Link>,
        });
      }

      if (userItems.length > 0) {
        menuItems.push({
          key: 'user-management-group',
          icon: <TeamOutlined style={{ fontSize: '18px' }} />,
          label: t('menu.users'),
          children: userItems
        });
      }
    }

    if (hasPermission('finance')) {
      const financeChildren = [];
      if (hasChildMenu('finance', 'finance.recharges')) {
        financeChildren.push({
          key: '/admin0755/finance/recharges',
          label: <Link to="/admin0755/finance/recharges">{t('menu.finance_recharges')}</Link>,
        });
      }
      if (hasChildMenu('finance', 'finance.orders')) {
        financeChildren.push({
          key: '/admin0755/finance/orders',
          label: <Link to="/admin0755/finance/orders">{t('menu.finance_orders')}</Link>,
        });
      }
      if (hasChildMenu('finance', 'finance.purchase_orders')) {
        financeChildren.push({
          key: '/admin0755/finance/purchase-orders',
          label: <Link to="/admin0755/finance/purchase-orders">{t('menu.finance_purchase_orders', '订单明细')}</Link>,
        });
      }
      if (hasChildMenu('finance', 'finance.gifts')) {
        financeChildren.push({
          key: '/admin0755/finance/gifts',
          label: <Link to="/admin0755/finance/gifts">{t('menu.finance_gifts')}</Link>,
        });
      }
      if (hasChildMenu('finance', 'finance.commissions')) {
        financeChildren.push({
          key: '/admin0755/finance/commissions',
          label: <Link to="/admin0755/finance/commissions">{t('menu.finance_commissions', '佣金明细')}</Link>,
        });
      }
      const isChinaInvoiceEnabled =
        settings?.invoices?.invoice_enabled !== false &&
        settings?.invoices?.invoice_mode === 'china';
      if (isChinaInvoiceEnabled && hasChildMenu('finance', 'finance.invoices')) {
        financeChildren.push({
          key: '/admin0755/finance/invoices',
          label: <Link to="/admin0755/finance/invoices">{t('menu.finance_invoices', '发票申请审核')}</Link>,
        });
      }
      if (hasChildMenu('finance', 'finance.analysis')) {
        financeChildren.push({
          key: '/admin0755/finance/analysis',
          label: <Link to="/admin0755/finance/analysis">{t('menu.finance_analysis')}</Link>,
        });
      }
      if (financeChildren.length > 0) {
        menuItems.push({
          key: 'finance-management-group',
          icon: <WalletOutlined style={{ fontSize: '18px' }} />,
          label: t('menu.finance'),
          children: financeChildren
        });
      }
    }

    if (hasPermission('settings')) {
      const settingsChildren = [];
      if (hasChildMenu('settings', 'settings.basic')) {
        settingsChildren.push({
          key: '/admin0755/settings?tab=basic',
          label: <Link to="/admin0755/settings?tab=basic">{t('menu.basic_settings')}</Link>,
        });
      }
      if (hasChildMenu('settings', 'settings.payment')) {
        settingsChildren.push({
          key: '/admin0755/payment-settings',
          label: <Link to="/admin0755/payment-settings">{t('menu.payment_settings')}</Link>,
        });
      }
      if (hasChildMenu('settings', 'settings.message_notification')) {
        settingsChildren.push({
          key: '/admin0755/message-notification',
          label: <Link to="/admin0755/message-notification">{t('menu.message_notification')}</Link>,
        });
      }
      if (hasChildMenu('settings', 'settings.oauth')) {
        settingsChildren.push({
          key: '/admin0755/oauth-settings',
          label: <Link to="/admin0755/oauth-settings">{t('menu.oauth_settings')}</Link>,
        });
      }
      if (hasChildMenu('settings', 'settings.database')) {
        settingsChildren.push({
          key: '/admin0755/settings?tab=database',
          label: <Link to="/admin0755/settings?tab=database">{t('menu.database_settings')}</Link>,
        });
      }
      if (settingsChildren.length > 0) {
        menuItems.push({
          key: 'settings-group',
          icon: <SettingOutlined style={{ fontSize: '18px' }} />,
          label: t('menu.settings'),
          children: settingsChildren
        });
      }
    }

    const hasAnyPluginPermission = isSuperAdmin || hasPermission('plugins') || user?.permissions?.some((p: string) => p.startsWith('plugin:'));
    if (hasAnyPluginPermission && import.meta.env.VITE_ENABLE_PLUGINS === 'true') {
      const hasGlobalPluginPerm = isSuperAdmin || hasPermission('plugins');
      const canSeePlugin = (pluginName: string) =>
        hasGlobalPluginPerm || !!user?.permissions?.includes(`plugin:${pluginName}`);
      const sidebarPlugins = selectAdminSidebarPlugins(adminPlugins, canSeePlugin);
      if (sidebarPlugins.length === 0) {
        menuItems.push({
          key: '/admin0755/plugins',
          icon: <AppstoreOutlined style={{ fontSize: '18px' }} />,
          label: <Link to="/admin0755/plugins">{t('menu.plugins')}</Link>,
        });
      } else {
        const pluginChildren = [
          {
            key: '/admin0755/plugins',
            label: <Link to="/admin0755/plugins">{t('menu.plugins_all')}</Link>,
          },
          ...sidebarPlugins.map((plugin) => ({
            key: `/admin0755/plugins/${plugin.name}/config`,
            label: (
              <Link to={adminSidebarPluginHref(plugin)}>
                {(() => {
                  const lang = (i18n.language || 'zh').toLowerCase();
                  const isSimplifiedZh = lang === 'zh' || lang === 'zh-cn' || lang === 'zh-hans' || lang === 'zh-sg';
                  if (!isSimplifiedZh && i18n.exists(`plugin_titles.${plugin.name}`)) {
                    return t(`plugin_titles.${plugin.name}`, plugin.title);
                  }
                  return adminSidebarPluginLabel(plugin, (name, title) => t(`plugin_titles.${name}`, title));
                })()}
              </Link>
            ),
          })),
        ];
        menuItems.push({
          key: 'plugins-group',
          icon: <AppstoreOutlined style={{ fontSize: '18px' }} />,
          label: t('menu.plugins'),
          children: pluginChildren,
        });
      }
    }
  }

  // 递归替换 /admin0755 为当前的 adminPath
  const processMenuItems = (items: any[]): any[] => {
    return items.map(item => {
      if (!item) return item;
      
      let newKey = item.key;
      if (typeof newKey === 'string' && newKey.startsWith('/admin0755')) {
        newKey = newKey.replace('/admin0755', `/${adminPath}`);
      }

      let newLabel = item.label;
      if (React.isValidElement(newLabel)) {
        const props = (newLabel as React.ReactElement<any>).props;
        if (props && typeof props.to === 'string' && props.to.startsWith('/admin0755')) {
          newLabel = React.cloneElement(newLabel as React.ReactElement<any>, {
            to: props.to.replace('/admin0755', `/${adminPath}`)
          });
        }
      }

      const newItem = {
        ...item,
        key: newKey,
        label: newLabel,
      };

      if (item.children) {
        newItem.children = processMenuItems(item.children);
      }

      return newItem;
    });
  };

  const processedMenuItems = processMenuItems(menuItems);

  const findSelectedKey = (items: any[]): string => {
    const fullPath = location.pathname + location.search;
    const path = location.pathname;

    // 1. Exact match with fullPath (search)
    for (const item of items) {
      if (item?.children) {
        for (const child of item.children) {
          if (child?.key === fullPath) return child.key;
        }
      } else if (item?.key === fullPath) {
        return item.key;
      }
    }

    // 2. Exact match with path
    for (const item of items) {
      if (item?.children) {
        for (const child of item.children) {
          if (child?.key === path) return child.key;
        }
      } else if (item?.key === path) {
        return item.key;
      }
    }

    // 3. Prefix match with path
    for (const item of items) {
      if (item?.children) {
        for (const child of item.children) {
          const baseKey = typeof child?.key === 'string' ? child.key.split('?')[0] : '';
          if (baseKey && baseKey !== '/' && path.startsWith(baseKey + '/')) {
            return child.key;
          }
        }
      } else {
        const baseKey = typeof item?.key === 'string' ? item.key.split('?')[0] : '';
        if (baseKey && baseKey !== '/' && path.startsWith(baseKey + '/')) {
          return item.key;
        }
      }
    }

    if (path.includes('/admin-groups')) {
      for (const item of items) {
        const children = item?.children || [];
        for (const child of children) {
          const baseKey = typeof child?.key === 'string' ? child.key.split('?')[0] : '';
          if (baseKey.endsWith('/user-levels')) return child.key;
        }
      }
    }

    return fullPath;
  };

  let pageName = '';
  const findName = (items: any[], checkPrefix = false): string | undefined => {
    const fullPath = location.pathname + location.search;
    const path = location.pathname;
    for (const item of items) {
      if (!item) continue;
      const isMatch = checkPrefix
        ? (typeof item.key === 'string' && item.key.split('?')[0] !== '/' && (path === item.key.split('?')[0] || path.startsWith(item.key.split('?')[0] + '/')))
        : (item.key === path || item.key === fullPath);

      if (isMatch) {
        if (typeof item.label === 'string') return item.label;
        if (item.label?.props?.children) {
          if (typeof item.label.props.children === 'string') return item.label.props.children;
          if (Array.isArray(item.label.props.children)) return item.label.props.children.join('');
        }
      }
      if (item.children) {
        const found = findName(item.children, checkPrefix);
        if (found) return found;
      }
    }
    return undefined;
  };

  const getActiveOpenKeys = () => {
    const fullPath = location.pathname + location.search;
    const path = location.pathname;
    const keys = processedMenuItems
      .filter((item: any) => item?.children?.some((child: any) => {
        if (child?.key === path || child?.key === fullPath) return true;
        const baseKey = typeof child?.key === 'string' ? child.key.split('?')[0] : '';
        return baseKey && baseKey !== '/' && path.startsWith(baseKey + '/');
      }))
      .map((item: any) => item.key as string);
    
    if (path.includes('/admin-groups') && !keys.includes('user-management-group')) {
      keys.push('user-management-group');
    }
    if (!keys.includes('user-settings-group')) {
      keys.push('user-settings-group');
    }
    return keys;
  };

  const adminPermsKnown =
    isUserEnd ||
    !user?.admin_group_id ||
    Array.isArray(user?.permissions) ||
    bootstrapDone;
  const menuReady = isUserEnd
    ? !!settings && userPluginsReady
    : adminPluginsReady && adminPermsKnown;

  useLayoutEffect(() => {
    if (!collapsed && menuReady) {
      const activeKeys = getActiveOpenKeys();
      // 站点控制台：切到其他菜单时保留已展开分组，只补当前页所在分组。收起仍由再次点击该分组完成。
      if (!isUserEnd) {
        setOpenKeys((prev) => {
          const merged = prev.slice();
          for (const key of activeKeys) {
            if (!merged.includes(key)) merged.push(key);
          }
          if (merged.length === prev.length) return prev;
          return merged;
        });
        return;
      }
      setOpenKeys(activeKeys);
    }
  }, [collapsed, location.pathname, location.search, activePlugins.length, adminPlugins, menuReady, settings, isUserEnd]);

  pageName = findName(processedMenuItems) || findName(processedMenuItems, true) || '';
  if (!pageName) {
    if (location.pathname === '/profile') pageName = t('menu.profile', '个人中心') as string;
    else if (location.pathname === '/wallet' || location.pathname === '/affiliate') pageName = t('menu.wallet', '我的钱包') as string;
    else if (location.pathname === '/assets') pageName = t('menu.assets', '资产素材') as string;
    else if (location.pathname === '/assets-intl') pageName = t('menu.assets_intl', '资产管理') as string;
    else if (location.pathname === '/advanced-marketing/model-usage') pageName = t('menu.referral_model_usage', '推广用户模型消费') as string;
    else if (location.pathname === '/advanced-marketing') pageName = t('menu.advanced_marketing', '团队营销管理') as string;
    else if (location.pathname.includes('/admin-groups')) pageName = t('menu.user_levels') as string;

    else if (location.pathname === '/playground' || location.pathname.startsWith('/playground/')) {
      const pgItem = settings?.menu_config?.items?.find((i: any) => i.key === '/playground');
      pageName = pgItem ? getMenuLabel(pgItem) : (i18n.exists('playground:title') ? t('playground:title') : t('menu.playground', '创作中心')) as string;
    }
    else if (location.pathname === '/playground-2026' || location.pathname.startsWith('/playground-2026/')) {
      const pg2026Item = settings?.menu_config?.items?.find((i: any) => i.key === '/playground-2026');
      pageName = pg2026Item ? getMenuLabel(pg2026Item) : (i18n.exists('playground_2026:title') ? t('playground_2026:title') : t('menu.playground_2026', '创作中心2026')) as string;
    }
  }

  useEffect(() => {
    if (pageName && siteTitle) {
      document.title = `${pageName}-${siteTitle}`;
    } else if (siteTitle) {
      document.title = siteTitle;
    }
  }, [pageName, siteTitle]);

  const isLight = themeMode === 'light';
  const borderBottom = isLight ? '1px solid #f0f0f0' : '1px solid rgba(255,255,255,0.08)';
  const cardBg = isLight ? '#f9fafb' : 'rgba(255, 255, 255, 0.04)';
  const cardHoverBg = isLight ? '#f3f4f6' : 'rgba(255, 255, 255, 0.08)';
  const titleColor = isLight ? '#1f2937' : '#fff';
  const timeColor = isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.4)';
  const contentColor = isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.7)';
  const emptyIconColor = isLight ? '#e5e7eb' : 'rgba(255,255,255,0.1)';
  const emptyTextColor = isLight ? '#6b7280' : '#e5e5e5';
  const emptySubtextColor = isLight ? '#9ca3af' : 'rgba(255,255,255,0.45)';

  const siteIntro = site?.intro?.trim();
  const renderAdminBadge = () => {
    const badge = (
      <span
        className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium leading-none shrink-0 border select-none transition-colors"
        style={{
          backgroundColor: isLight ? 'rgba(0, 0, 0, 0.04)' : 'rgba(255, 255, 255, 0.08)',
          borderColor: isLight ? 'rgba(0, 0, 0, 0.08)' : 'rgba(255, 255, 255, 0.12)',
          color: isLight ? '#71717a' : '#a1a1aa',
        }}
      >
        {t('common.admin_suffix', '管理后台').trim()}
      </span>
    );

    if (!siteIntro) {
      return badge;
    }

    return (
      <Tooltip
        title={
          <div style={{ maxWidth: 280, fontSize: 12, lineHeight: 1.6, padding: '2px 0', wordBreak: 'break-word', whiteSpace: 'pre-wrap' }}>
            {siteIntro}
          </div>
        }
        placement="bottom"
      >
        {badge}
      </Tooltip>
    );
  };

  const announcementContent = (
    <div style={{ width: 360, display: 'flex', flexDirection: 'column' }}>
      <div style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        padding: '16px 20px', borderBottom
      }}>
        <span style={{ color: titleColor, fontSize: 16, fontWeight: 500 }}>{t('header.notifications', '通知')}</span>
      </div>

      <div style={{
        maxHeight: 480, overflowY: 'auto', padding: announcements.length > 0 ? '16px' : '60px 20px',
        display: 'flex', flexDirection: 'column',
      }}>
        {announcements.length > 0 ? (
          <List
            itemLayout="vertical"
            dataSource={announcements}
            split={false}
            renderItem={(item) => (
              <div
                key={item.id}
                style={{
                  background: cardBg,
                  borderRadius: 12,
                  padding: '16px',
                  marginBottom: 12,
                  transition: 'all 0.3s cubic-bezier(0.4, 0, 0.2, 1)',
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = cardHoverBg;
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = cardBg;
                }}
              >
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 12 }}>
                  <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
                    {item.is_pinned === 1 && (
                      <div style={{
                        ...softAccent(themeMode === 'light' ? 'light' : 'dark'),
                        fontSize: 12,
                        padding: '2px 6px', borderRadius: 4, marginTop: 2, whiteSpace: 'nowrap',
                        flexShrink: 0
                      }}>
                        {t('common.pinned', '置顶')}
                      </div>
                    )}
                    <div style={{ color: titleColor, fontSize: 15, fontWeight: 500, lineHeight: 1.5 }}>
                      {getAnnouncementLabel(item.title)}
                    </div>
                  </div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: timeColor, fontSize: 12 }}>
                    <ScheduleOutlined />
                    {new Date(getAnnouncementDisplayTime(item)).toLocaleString(toDisplayLocale(i18n.language), {
                      year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit'
                    })}
                  </div>
                </div>

                <div
                  className="quill-content"
                  dangerouslySetInnerHTML={{ __html: getAnnouncementLabel(item.content) }}
                  style={{
                    color: contentColor, fontSize: 13, lineHeight: 1.6,
                    background: 'transparent', padding: '0', overflowWrap: 'break-word', wordBreak: 'break-all'
                  }}
                />
              </div>
            )}
          />
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', textAlign: 'center' }}>
            <BellOutlined style={{ fontSize: 64, color: emptyIconColor, marginBottom: 24 }} />
            <div style={{ color: emptyTextColor, fontSize: 15, fontWeight: 500, marginBottom: 8 }}>{t('header.no_notifications', '你的通知将出现在这里')}</div>
            <div style={{ color: emptySubtextColor, fontSize: 13, lineHeight: 1.6, maxWidth: 260 }}>
              {t('header.no_notifications_desc', '平台重要公告及更新内容将在这里展示，即可第一时间收到通知。')}
            </div>
          </div>
        )}
      </div>
    </div>
  );

  return (
    <ConfigProvider
      theme={{
        token: getAntdThemeTokens(themeMode === 'light' ? 'light' : 'dark'),
        components: {
          Layout: {
            /* siderBg handled by global */
          },
          Menu: {
            ...getSiderMenuTokens(themeMode === 'light' ? 'light' : 'dark'),
            itemHeight: 50,
            iconSize: 20,
            itemMarginInline: 12,
          }
        }
      }}
    >
      <Layout style={{ height: '100vh', overflow: 'hidden' }}>
        <Sider
          trigger={null}
          collapsible
          collapsed={collapsed}
          theme={themeMode}
          width={240}
          collapsedWidth={screens.xs ? 0 : 68}
          style={{
            boxShadow: 'none',
            borderRight: themeMode === 'light' ? '1px solid #e4e4e7' : '1px solid #1f1f23',
            zIndex: screens.xs ? 1050 : 10,
            position: screens.xs ? 'fixed' : 'relative',
            height: '100%',
            left: 0,
            top: 0,
            bottom: 0,
            overflow: 'hidden',
          }}
          className="custom-sider"
        >
          <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
            {/* Logo Area：展开态固定宽度裁切 + 与图标轨交叉淡入，避免文字挤压 */}
            <div
              style={{
                height: screens.xs ? 48 : 56,
                position: 'relative',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                padding: '0 8px',
                borderBottom: themeMode === 'light' ? '1px solid #e4e4e7' : '1px solid #1f1f23',
                overflow: 'hidden',
                flexShrink: 0,
              }}
            >
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 6,
                  justifyContent: 'center',
                  width: 224,
                  minWidth: 224,
                  maxWidth: 224,
                  opacity: collapsed ? 0 : 1,
                  transition: collapsed
                    ? 'opacity 0.1s ease'
                    : 'opacity 0.18s ease 0.12s',
                  pointerEvents: collapsed ? 'none' : 'auto',
                  ...(logoTitleHref ? { cursor: 'pointer', textDecoration: 'none', color: 'inherit' } : {}),
                }}
                {...(logoTitleHref
                  ? {
                      role: 'link',
                      tabIndex: 0,
                      onClick: goLogoTitle,
                      onKeyDown: (e: React.KeyboardEvent) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          goLogoTitle();
                        }
                      },
                    }
                  : {})}
              >
                {siteLogo ? (
                  <img src={siteLogo} alt="logo" style={{ width: 20, height: 20, objectFit: 'contain', flexShrink: 0 }} />
                ) : (
                  <div className="flex items-center justify-center w-5 h-5 rounded bg-primary text-primary-foreground flex-shrink-0">
                    <Terminal className="w-3 h-3" />
                  </div>
                )}
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    minWidth: 0,
                    flexShrink: 1,
                    overflow: 'hidden',
                  }}
                >
                  <div
                    style={{
                      color: themeMode === 'light' ? '#1f2937' : '#fff',
                      margin: 0,
                      fontSize: siteName.length > 14 ? 12 : siteName.length > 10 ? 13 : siteName.length > 6 ? 14 : 16,
                      fontWeight: 700,
                      lineHeight: 1.2,
                      wordBreak: 'break-word',
                      minWidth: 0,
                    }}
                    title={siteName}
                  >
                    {siteName}
                  </div>
                  {!isUserEnd && renderAdminBadge()}
                </div>
              </div>
              {!screens.xs && (
                <div
                  style={{
                    position: 'absolute',
                    inset: 0,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    opacity: collapsed ? 1 : 0,
                    transition: collapsed
                      ? 'opacity 0.18s ease 0.12s'
                      : 'opacity 0.1s ease',
                    pointerEvents: collapsed ? 'auto' : 'none',
                    ...(logoTitleHref ? { cursor: 'pointer' } : {}),
                  }}
                  {...(logoTitleHref
                    ? {
                        role: 'link',
                        tabIndex: 0,
                        onClick: goLogoTitle,
                        onKeyDown: (e: React.KeyboardEvent) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            goLogoTitle();
                          }
                        },
                      }
                    : {})}
                >
                  {siteLogo ? (
                    <img src={siteLogo} alt="logo" style={{ width: 24, height: 24, objectFit: 'contain' }} />
                  ) : (
                    <div className="flex items-center justify-center w-6 h-6 rounded-md bg-primary text-primary-foreground">
                      <Terminal className="w-3.5 h-3.5" />
                    </div>
                  )}
                </div>
              )}
            </div>
            <div style={{ flex: 1, overflowY: 'auto', overflowX: 'hidden' }}>
              <ConfigProvider
                theme={{
                  components: {
                    Menu: getSiderMenuTokens(themeMode === 'light' ? 'light' : 'dark'),
                  }
                }}
              >
                {menuReady && (
                <Menu
                  theme={themeMode}
                  mode="inline"
                  selectedKeys={[findSelectedKey(processedMenuItems)]}
                  openKeys={collapsed ? undefined : openKeys}
                  onOpenChange={(keys) => {
                    if (!collapsed) {
                      setOpenKeys(keys);
                    }
                  }}
                  items={processedMenuItems}
                  expandIcon={({ isOpen }) => (
                    <RightOutlined
                      className="ant-menu-submenu-expand-icon"
                      rotate={isOpen ? 90 : 0}
                      style={{ fontSize: 10 }}
                    />
                  )}
                  style={{ border: 'none', background: 'transparent', marginTop: 8 }}
                  onClick={() => {
                    if (screens.xs) setCollapsed(true);
                  }}
                />
                )}
              </ConfigProvider>
            </div>
            {!isUserEnd && (
              <div style={{ padding: '16px 8px', borderTop: '1px solid rgba(255,255,255,0.05)', display: 'flex', justifyContent: 'center' }}>
                <Button
                  type="text"
                  icon={<InfoCircleOutlined style={{ fontSize: '18px' }} />}
                  style={{ color: themeMode === 'light' ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', width: '100%', display: 'flex', alignItems: 'center', justifyContent: collapsed ? 'center' : 'flex-start' }}
                  onClick={showSystemAbout}
                  title={`${t('menu.system_about')}${settings?.is_open_source ? t('menu.edition_oss') : t('menu.edition_commercial')}`}
                >
                  {(!collapsed) && <span style={{ marginLeft: 8 }}>{t('menu.system_about')}{settings?.is_open_source ? t('menu.edition_oss') : t('menu.edition_commercial')}</span>}
                </Button>
              </div>
            )}
          </div>
        </Sider>
        <Layout style={{
          marginLeft: (screens.xs || collapsed) ? 0 : 0,
          background: 'var(--dashboard)',
          position: 'relative',
          overflow: 'hidden',
          display: 'flex',
          flexDirection: 'column',
        }}>
          <Header
            className="dashboard-top-nav-glass"
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              right: 0,
              zIndex: 100,
              padding: '0 12px',
              background: isLight ? 'rgba(255, 255, 255, 0.62)' : 'rgba(0, 0, 0, 0.48)',
              backdropFilter: 'blur(18px) saturate(180%)',
              WebkitBackdropFilter: 'blur(18px) saturate(180%)',
              height: screens.xs ? 48 : 56,
              lineHeight: (screens.xs ? 48 : 56) + 'px',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              paddingRight: screens.xs ? 8 : 24,
              borderBottom: isLight
                ? '1px solid rgba(228, 228, 231, 0.45)'
                : '1px solid rgba(255, 255, 255, 0.08)',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', minWidth: 0, flexShrink: 1, overflow: 'hidden' }}>
              <Button
                type="text"
                icon={<SidebarIcon size={16} />}
                onClick={() => handleCollapsedChange(!collapsed)}
                style={{
                  width: 32,
                  height: 32,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  color: themeMode === 'light' ? '#71717a' : '#a1a1aa',
                  borderRadius: 6,
                  flexShrink: 0,
                }}
              />
              {screens.xs && (
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    marginLeft: 6,
                    minWidth: 0,
                    overflow: 'hidden',
                    cursor: 'pointer'
                  }}
                  onClick={() => {
                    if (logoTitleHref) {
                      goLogoTitle();
                    } else {
                      navigate('/dashboard');
                    }
                  }}
                >
                  {siteLogo ? (
                    <img src={siteLogo} alt="logo" style={{ width: 18, height: 18, objectFit: 'contain', flexShrink: 0 }} />
                  ) : (
                    <div className="flex items-center justify-center w-4.5 h-4.5 rounded bg-primary text-primary-foreground flex-shrink-0">
                      <Terminal className="w-3 h-3" />
                    </div>
                  )}
                  <span
                    style={{
                      color: themeMode === 'light' ? '#1f2937' : '#fff',
                      margin: 0,
                      fontSize: siteName.length > 14 ? 11 : siteName.length > 10 ? 12 : 13,
                      fontWeight: 600,
                      whiteSpace: 'nowrap',
                      lineHeight: 1.2,
                    }}
                    title={siteName}
                  >
                    {siteName}
                  </span>
                  {!isUserEnd && renderAdminBadge()}
                </div>
              )}
            </div>

            <div
              className="header-nav-actions"
              style={{ position: 'relative', display: 'flex', alignItems: 'center', gap: screens.xs ? 2 : 6, flexShrink: 0, lineHeight: 1 }}
              ref={(container) => {
                if (!container) return;
                requestAnimationFrame(() => {
                  if (!container.isConnected) return;
                  const pill = container.querySelector('.header-nav-hover-pill') as HTMLElement | null;
                  if (!pill) return;
                  const pt = (window as any).__tkeHeaderPointer as { x: number; y: number } | undefined;
                  const hovered = (
                    (pt ? document.elementFromPoint(pt.x, pt.y)?.closest('.ant-btn') : null) ||
                    container.querySelector('.ant-btn:hover')
                  ) as HTMLElement | null;
                  if (hovered && container.contains(hovered)) {
                    const cRect = container.getBoundingClientRect();
                    const bRect = hovered.getBoundingClientRect();
                    if (bRect.width > 0 && bRect.height > 0) {
                      const x = bRect.left - cRect.left;
                      const y = bRect.top - cRect.top;
                      const isCircle = hovered.classList.contains('ant-btn-circle');
                      const radius = isCircle ? `${bRect.height / 2}px` : '10px';
                      const baseTransform = `translate3d(${x}px, ${y}px, 0)`;
                      pill.dataset.lastTransform = baseTransform;
                      if (pill.dataset.visible !== '1') {
                        pill.style.transition = 'none';
                      }
                      pill.style.width = `${bRect.width}px`;
                      pill.style.height = `${bRect.height}px`;
                      pill.style.borderRadius = radius;
                      pill.style.transform = `${baseTransform} scale(1)`;
                      pill.style.opacity = '1';
                      pill.dataset.visible = '1';
                    }
                  }
                });
              }}
              onMouseDown={(e) => {
                (window as any).__tkeHeaderPointer = { x: e.clientX, y: e.clientY };
              }}
              onMouseMove={(e) => {
                (window as any).__tkeHeaderPointer = { x: e.clientX, y: e.clientY };
              }}
              onMouseOver={(e) => {
                (window as any).__tkeHeaderPointer = { x: e.clientX, y: e.clientY };
                const container = e.currentTarget;
                const pill = container.querySelector('.header-nav-hover-pill') as HTMLElement | null;
                if (!pill) return;
                const btn = (e.target as HTMLElement).closest('.ant-btn') as HTMLElement | null;
                if (!btn || !container.contains(btn)) {
                  if ((e.target as HTMLElement).closest('.header-avatar-wrap')) {
                    pill.dataset.visible = '0';
                    pill.style.opacity = '0';
                    if (pill.dataset.lastTransform) {
                      pill.style.transform = `${pill.dataset.lastTransform} scale(0.92)`;
                    }
                  }
                  return;
                }
                const cRect = container.getBoundingClientRect();
                const bRect = btn.getBoundingClientRect();
                const x = bRect.left - cRect.left;
                const y = bRect.top - cRect.top;
                const isCircle = btn.classList.contains('ant-btn-circle');
                const radius = isCircle ? `${bRect.height / 2}px` : '10px';
                const baseTransform = `translate3d(${x}px, ${y}px, 0)`;
                pill.dataset.lastTransform = baseTransform;
                if (pill.dataset.visible !== '1') {
                  pill.style.transition = 'none';
                  pill.style.width = `${bRect.width}px`;
                  pill.style.height = `${bRect.height}px`;
                  pill.style.borderRadius = radius;
                  pill.style.transform = `${baseTransform} scale(0.92)`;
                  void pill.offsetWidth;
                  pill.style.transition = 'transform 0.32s cubic-bezier(0.22, 1, 0.36, 1), width 0.32s cubic-bezier(0.22, 1, 0.36, 1), height 0.32s cubic-bezier(0.22, 1, 0.36, 1), border-radius 0.32s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.2s ease';
                  pill.style.transform = `${baseTransform} scale(1)`;
                  pill.style.opacity = '1';
                  pill.dataset.visible = '1';
                } else {
                  pill.style.transition = 'transform 0.32s cubic-bezier(0.22, 1, 0.36, 1), width 0.32s cubic-bezier(0.22, 1, 0.36, 1), height 0.32s cubic-bezier(0.22, 1, 0.36, 1), border-radius 0.32s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.2s ease';
                  pill.style.width = `${bRect.width}px`;
                  pill.style.height = `${bRect.height}px`;
                  pill.style.borderRadius = radius;
                  pill.style.transform = `${baseTransform} scale(1)`;
                  pill.style.opacity = '1';
                }
              }}
              onMouseLeave={(e) => {
                (window as any).__tkeHeaderPointer = { x: e.clientX, y: e.clientY };
                const pill = e.currentTarget.querySelector('.header-nav-hover-pill') as HTMLElement | null;
                if (!pill) return;
                pill.dataset.visible = '0';
                pill.style.opacity = '0';
                if (pill.dataset.lastTransform) {
                  pill.style.transform = `${pill.dataset.lastTransform} scale(0.92)`;
                }
              }}
            >
              <style>{`
                .header-badge.ant-badge {
                  display: flex !important;
                  align-items: center;
                  justify-content: center;
                  height: 40px;
                }
                .header-nav-hover-pill {
                  position: absolute;
                  top: 0;
                  left: 0;
                  opacity: 0;
                  pointer-events: none;
                  z-index: 0;
                  will-change: transform, width, height, border-radius, opacity;
                  background: ${
                    themeMode === 'light'
                      ? 'rgba(0, 0, 0, 0.075)'
                      : 'rgba(255, 255, 255, 0.085)'
                  };
                  backdrop-filter: blur(10px) saturate(150%);
                  -webkit-backdrop-filter: blur(10px) saturate(150%);
                }
                .header-nav-actions .ant-btn {
                  position: relative;
                  z-index: 1;
                  display: inline-flex !important;
                  align-items: center !important;
                  justify-content: center !important;
                  line-height: 1 !important;
                  background: transparent !important;
                  transition: color 0.22s ease, transform 0.22s ease !important;
                }
                .header-nav-actions .ant-btn:hover,
                .header-nav-actions .ant-btn:focus,
                .header-nav-actions .ant-btn:active {
                  background: transparent !important;
                }
                .header-nav-actions .ant-btn .ant-btn-icon {
                  display: inline-flex !important;
                  align-items: center !important;
                  justify-content: center !important;
                  line-height: 1 !important;
                  margin-inline-end: 0 !important;
                }
                .header-nav-actions .ant-btn > span:not(.ant-btn-icon) {
                  display: inline-flex !important;
                  align-items: center !important;
                  line-height: 1 !important;
                  margin-inline-start: 0 !important;
                }
                .header-nav-actions .ant-btn svg {
                  display: block;
                  transition: transform 0.28s cubic-bezier(0.34, 1.56, 0.64, 1);
                }
                .header-nav-actions .ant-btn:hover svg {
                  transform: scale(1.06);
                }
                .header-nav-actions .ant-btn:active svg {
                  transform: scale(0.95);
                }
              `}</style>
              <div className="header-nav-hover-pill" />
              <div style={{ display: 'flex', alignItems: 'center', gap: 2 }}>
                {isUserEnd && couponCampaign && (
                  <Button
                    type="text"
                    icon={
                      <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        xmlns="http://www.w3.org/2000/svg"
                      >
                        <path d="M3 7.5C3 6.11929 4.11929 5 5.5 5H18.5C19.8807 5 21 6.11929 21 7.5V8.6H3V7.5Z" fill={themeMode === 'light' ? '#9e9e9e' : '#7a7a7a'} />
                        <path d="M3 8.6H21V11.1H3V8.6Z" fill={themeMode === 'light' ? '#e0e0e0' : '#2e2e2e'} />
                        <path d="M3 11.1H21V16.5C21 17.8807 19.8807 19 18.5 19H5.5C4.11929 19 3 17.8807 3 16.5V11.1Z" fill={themeMode === 'light' ? '#757575' : '#9e9e9e'} />
                        <rect x="5.5" y="13.3" width="4.2" height="3" rx="0.8" fill={themeMode === 'light' ? '#e0e0e0' : '#2e2e2e'} />
                        <path d="M12.2 14.8H18" stroke={themeMode === 'light' ? '#b0b0b0' : '#555555'} strokeWidth="1.6" strokeLinecap="round" />
                      </svg>
                    }
                    style={{
                      color: themeMode === 'light' ? '#1f2937' : '#fff',
                      gap: 4,
                      fontSize: 14,
                      fontWeight: 500,
                      height: screens.xs ? 34 : 40,
                      width: screens.xs ? 34 : undefined,
                      padding: screens.xs ? 0 : '0 10px 0 7px',
                    }}
                    onClick={() => navigate(couponCampaignPath(couponCampaign.template_key, couponCampaign.id), {
                      state: offerReturnState(`${location.pathname}${location.search}${location.hash}`),
                    })}
                  >
                    {!screens.xs && (
                      <span>{t('menu.coupon_records', '卡券订阅')}</span>
                    )}
                  </Button>
                )}
                {isPluginVisibleForUser('model_marketplace') && (
                  <Button
                    type="text"
                    href="/home/models"
                    icon={
                      <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        xmlns="http://www.w3.org/2000/svg"
                      >
                        <path d="M12 2.5L19.5 6.5L12 10.5L4.5 6.5Z" fill={themeMode === 'light' ? '#e0e0e0' : '#424242'} />
                        <path d="M3.5 8L11 12V20.5L3.5 16.5Z" fill={themeMode === 'light' ? '#b0b0b0' : '#555555'} />
                        <path d="M13 12L20.5 8V16.5L13 20.5Z" fill={themeMode === 'light' ? '#757575' : '#9e9e9e'} />
                      </svg>
                    }
                    style={{
                      color: themeMode === 'light' ? '#1f2937' : '#fff',
                      gap: 4,
                      fontSize: 14,
                      fontWeight: 500,
                      height: screens.xs ? 34 : 40,
                      width: screens.xs ? 34 : undefined,
                      padding: screens.xs ? 0 : '0 10px 0 7px',
                    }}
                    onClick={(e) => {
                      if (!e.metaKey && !e.ctrlKey) {
                        e.preventDefault();
                        navigate('/home/models');
                      }
                    }}
                  >
                    {!screens.xs && (
                      <span>{t('menu.model_marketplace', 'Models')}</span>
                    )}
                  </Button>
                )}

                {isPluginVisibleForUser('docs_api') && (
                  <Button
                    type="text"
                    href="/docs"
                    icon={
                      <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        xmlns="http://www.w3.org/2000/svg"
                      >
                        <rect x="8" y="2.5" width="2.6" height="5.8" rx="1.2" fill={themeMode === 'light' ? '#757575' : '#9e9e9e'} />
                        <rect x="13.4" y="2.5" width="2.6" height="5.8" rx="1.2" fill={themeMode === 'light' ? '#757575' : '#9e9e9e'} />
                        <path d="M4.5 7.5H19.5V10H4.5V7.5Z" fill={themeMode === 'light' ? '#e0e0e0' : '#2e2e2e'} />
                        <path d="M5 10H12V21C7.8 21 5 18.6 5 15.2V10Z" fill={themeMode === 'light' ? '#b0b0b0' : '#555555'} />
                        <path d="M12 10H19V15.2C19 18.6 16.2 21 12 21V10Z" fill={themeMode === 'light' ? '#757575' : '#9e9e9e'} />
                        <path d="M8.5 12.2V16.8" stroke={themeMode === 'light' ? '#757575' : '#2e2e2e'} strokeWidth="1.4" strokeLinecap="round" />
                        <path d="M15.5 12.2V16.8" stroke={themeMode === 'light' ? '#b0b0b0' : '#555555'} strokeWidth="1.4" strokeLinecap="round" />
                      </svg>
                    }
                    style={{
                      color: themeMode === 'light' ? '#1f2937' : '#fff',
                      gap: 4,
                      fontSize: 14,
                      fontWeight: 500,
                      height: screens.xs ? 34 : 40,
                      width: screens.xs ? 34 : undefined,
                      padding: screens.xs ? 0 : '0 10px 0 7px',
                    }}
                    onClick={(e) => {
                      if (!e.metaKey && !e.ctrlKey) {
                        e.preventDefault();
                        navigate('/docs');
                      }
                    }}
                  >
                    {!screens.xs && (
                      <span>{t('menu.relay_api', 'API教程')}</span>
                    )}
                  </Button>
                )}
              </div>

              {(enableThemeToggle || !isUserEnd) && (
                <Tooltip title={themeMode === 'light' ? t('header.switch_dark_mode', '切换暗色模式') : t('header.switch_light_mode', '切换亮色模式')} placement="bottom" color={themeMode === 'light' ? '#fff' : '#2b2b2b'} styles={{ container: { color: themeMode === 'light' ? '#1f2937' : '#fff' } }}>
                  <Button
                    type="text"
                    shape="circle"
                    onClick={toggleTheme}
                    icon={
                      themeMode === 'light'
                        ? (
                          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79Z" fill="#757575" />
                          </svg>
                        )
                        : (
                          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <circle cx="12" cy="12" r="6" fill="#555555" />
                            <path d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32l1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41m11.32-11.32l1.41-1.41" stroke="#9e9e9e" strokeWidth="2.2" strokeLinecap="round" />
                          </svg>
                        )
                    }
                    style={{ color: themeMode === 'light' ? '#1f2937' : '#fff', width: screens.xs ? 34 : 40, height: screens.xs ? 34 : 40, padding: 0 }}
                  />
                </Tooltip>
              )}

              {enableMultilingual && (
                <Dropdown menu={{ items: langItems }} placement="bottomRight">
                  <Button
                    type="text"
                    shape="circle"
                    icon={
                      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <circle cx="12" cy="12" r="8.5" stroke={themeMode === 'light' ? '#757575' : '#9e9e9e'} strokeWidth="2" />
                        <path d="M3.5 12h17" stroke={themeMode === 'light' ? '#b0b0b0' : '#555555'} strokeWidth="2" strokeLinecap="round" />
                        <ellipse cx="12" cy="12" rx="3.5" ry="8.5" stroke={themeMode === 'light' ? '#b0b0b0' : '#555555'} strokeWidth="2" />
                      </svg>
                    }
                    style={{ color: themeMode === 'light' ? '#1f2937' : '#fff', width: screens.xs ? 34 : 40, height: screens.xs ? 34 : 40, padding: 0 }}
                  />
                </Dropdown>
              )}

              {showTicketButton && (
                <Tooltip title={t('header.tickets', '工单')} placement="bottom" color={themeMode === 'light' ? '#fff' : '#2b2b2b'} styles={{ container: { color: themeMode === 'light' ? '#1f2937' : '#fff' } }}>
                  <Badge count={ticketUnread} overflowCount={99} offset={[-4, 4]} className="header-badge">
                    <Button
                      type="text"
                      shape="circle"
                      aria-label={t('header.tickets', '工单')}
                      icon={
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                          <path d="M4 7.5A2.5 2.5 0 016.5 5h11A2.5 2.5 0 0120 7.5v2a1.6 1.6 0 000 3.2v3.8A2.5 2.5 0 0117.5 19h-11A2.5 2.5 0 014 16.5v-3.8a1.6 1.6 0 000-3.2v-2z" fill={themeMode === 'light' ? '#757575' : '#9e9e9e'} />
                          <path d="M9 9.2h6M9 12h6M9 14.8h3.5" stroke={themeMode === 'light' ? '#fff' : '#2e2e2e'} strokeWidth="1.4" strokeLinecap="round" />
                        </svg>
                      }
                      style={{ color: themeMode === 'light' ? '#1f2937' : '#fff', width: screens.xs ? 34 : 40, height: screens.xs ? 34 : 40, padding: 0 }}
                      onClick={() => navigate('/tickets')}
                    />
                  </Badge>
                </Tooltip>
              )}

              <Popover
                content={announcementContent}
                trigger="click"
                placement="bottomRight"
                overlayClassName="custom-premium-popover"
                open={announcementsDrawerVisible}
                onOpenChange={setAnnouncementsDrawerVisible}
                styles={{ container: { padding: 0, background: 'transparent', boxShadow: 'none' } }}
                motion={{ motionName: '' }}
                arrow={false}
              >
                <Tooltip title={t('header.notifications', '通知')} placement="bottom" color={themeMode === 'light' ? '#fff' : '#2b2b2b'} styles={{ container: { color: themeMode === 'light' ? '#1f2937' : '#fff' } }}>
                  <Badge count={unreadCount} overflowCount={99} offset={[-4, 4]} className="header-badge">
                    <Button
                      type="text"
                      shape="circle"
                      icon={
                        <svg
                          width="20"
                          height="20"
                          viewBox="0 0 24 24"
                          fill="none"
                          xmlns="http://www.w3.org/2000/svg"
                        >
                          <path d="M19 16v-6a7 7 0 00-14 0v6l-2 2h18l-2-2z" fill={themeMode === 'light' ? '#757575' : '#9e9e9e'} stroke={themeMode === 'light' ? '#757575' : '#9e9e9e'} strokeWidth="1.5" strokeLinejoin="round" />
                          <path d="M10 19a2 2 0 004 0" stroke={themeMode === 'light' ? '#b0b0b0' : '#555555'} strokeWidth="2.5" strokeLinecap="round" />
                        </svg>
                      }
                      style={{ color: themeMode === 'light' ? '#1f2937' : '#fff', width: screens.xs ? 34 : 40, height: screens.xs ? 34 : 40, padding: 0 }}
                      onClick={() => {
                        setUnreadCount(0);
                      }}
                    />
                  </Badge>
                </Tooltip>
              </Popover>

              <div className="header-avatar-wrap" style={{ display: 'flex', alignItems: 'center' }}>
                <UserAvatarMenu isUserEnd={isUserEnd} agreement={agreement} />
              </div>
            </div>

          </Header>
          <Content
            ref={contentRef}
            style={{
            position: 'relative',
            zIndex: 1,
            margin: screens.xs ? '0 8px 8px' : '0 12px 12px',
            // 顶栏浮层 + 10px 间距，滚动时内容穿过毛玻璃
            padding: screens.xs ? 12 : 16,
            paddingTop: (screens.xs ? 48 : 56) + 10,
            minHeight: 280,
            background: 'transparent',
            borderRadius: 8,
            overflow: 'auto',
            flex: 1,
            height: '100%',
            display: 'flex',
            flexDirection: 'column',
          }}>
            <div style={{ flex: '1 0 auto', width: '100%', minWidth: 0 }}>
              <ErrorBoundary>
                <Outlet context={outletContextValue} />
              </ErrorBoundary>
            </div>
            {site?.copyright && (
              <div
                style={{
                  flexShrink: 0,
                  paddingTop: screens.xs ? 16 : 24,
                  paddingBottom: screens.xs ? 4 : 8,
                  textAlign: 'center',
                  fontSize: 12,
                  lineHeight: 1.5,
                  color: isLight ? 'rgba(0, 0, 0, 0.45)' : 'rgba(255, 255, 255, 0.35)',
                  userSelect: 'none',
                }}
              >
                {site.copyright}
              </div>
            )}
          </Content>
          {screens.xs && !collapsed && (
            <div
              style={{
                position: 'fixed',
                top: 0,
                left: 0,
                right: 0,
                bottom: 0,
                background: 'rgba(0,0,0,0.5)',
                zIndex: 1040,
              }}
              onClick={() => setCollapsed(true)}
            />
          )}
        </Layout>
      </Layout>

      {/* 登录控制台弹窗通知 (Shadcn UI Style) */}
      <AnnouncementPopupModal
        announcements={popupAnnouncements}
        open={popupModalOpen}
        onClose={handleClosePopupModal}
        themeMode={themeMode === 'light' ? 'light' : 'dark'}
      />

      {isUserEnd && <BindPromptModal />}
    </ConfigProvider>
  );
};

export default DashboardLayout;

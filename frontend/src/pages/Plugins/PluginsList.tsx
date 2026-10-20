/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect } from 'react';
import { Switch, message, Spin } from 'antd';
import { 
  Image, 
  Users, 
  Beaker, 
  Cloud, 
  Store, 
  LayoutGrid, 
  Network, 
  Home, 
  BookOpen, 
  Zap, 
  MonitorPlay,
  Settings,
  Globe,
  Share2,
  ArrowLeftRight,
  Video,
  LifeBuoy,
  History,
  Gamepad2,
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import request from '../../utils/request';
import { invalidateActivePluginsCache } from '../../utils/activePlugins';
import { invalidateAdminPluginsCache } from '../../utils/adminPlugins';
import type { Plugin } from '../../types';
import { pluginLocales } from '../../i18n';
import useSettingsStore from '../../store/settings';
import { useTranslation } from 'react-i18next';
import useAuthStore from '../../store/auth';

/** 语言代码 → 显示名称 */
const langNames: Record<string, string> = {
  zh: '中文',
  'zh-TW': '繁體中文',
  en: 'English',
  ja: '日本語',
  ko: '한국어',
  vi: 'Tiếng Việt',
  fr: 'Français',
  de: 'Deutsch',
  es: 'Español',
  pt: 'Português',
};

// ── 插件图标映射 ──
const pluginIcons: Record<string, React.ReactNode> = {
  asset_manager: <Image className="w-3.5 h-3.5" />,
  asset_manager_intl: <Image className="w-3.5 h-3.5" />,
  team_marketing: <Users className="w-3.5 h-3.5" />,
  playground: <Beaker className="w-3.5 h-3.5" />,
  playground_2026: <Beaker className="w-3.5 h-3.5" />,
  model_marketplace: <Store className="w-3.5 h-3.5" />,
  site_icons: <LayoutGrid className="w-3.5 h-3.5" />,

  site_portal: <Home className="w-3.5 h-3.5" />,
  site_portal_pro: <Home className="w-3.5 h-3.5" />,
  docs_api: <BookOpen className="w-3.5 h-3.5" />,
  volcengine_ark_monitor: <MonitorPlay className="w-3.5 h-3.5" />,
  upstream_asset_relay: <Share2 className="w-3.5 h-3.5" />,
  comfyui_bridge: <Video className="w-3.5 h-3.5" />,
  data_sync: <ArrowLeftRight className="w-3.5 h-3.5" />,
  support_tickets: <LifeBuoy className="w-3.5 h-3.5" />,
  logs_archive: <History className="w-3.5 h-3.5" />,
  marketing_games: <Gamepad2 className="w-3.5 h-3.5" />,
};

// 系统增强插件使用不同的图标/徽章颜色样式，适配 shadcn ui 风格
const categoryStyles: Record<string, {
  iconContainer: string;
  badge: string;
  cardBorder: string;
}> = {
  user: {
    iconContainer: 'bg-blue-50 dark:bg-blue-950/20 text-blue-600 dark:text-blue-400 border border-blue-150/40 dark:border-blue-900/30',
    badge: 'bg-blue-50 dark:bg-blue-950/20 text-blue-600 dark:text-blue-400 border border-blue-100/30 dark:border-blue-900/20',
    cardBorder: 'hover:border-blue-400/50 dark:hover:border-blue-500/30',
  },
  system: {
    iconContainer: 'bg-amber-50 dark:bg-amber-950/20 text-amber-600 dark:text-amber-400 border border-amber-150/40 dark:border-amber-900/30',
    badge: 'bg-amber-50 dark:bg-amber-950/20 text-amber-600 dark:text-amber-400 border border-amber-100/30 dark:border-amber-900/20',
    cardBorder: 'hover:border-amber-400/50 dark:hover:border-amber-500/30',
  },
  system_builtin: {
    iconContainer: 'bg-emerald-50 dark:bg-emerald-950/20 text-emerald-600 dark:text-emerald-400 border border-emerald-150/40 dark:border-emerald-900/30',
    badge: 'bg-emerald-50 dark:bg-emerald-950/20 text-emerald-600 dark:text-emerald-400 border border-emerald-100/30 dark:border-emerald-900/20',
    cardBorder: 'hover:border-emerald-400/50 dark:hover:border-emerald-500/30',
  }
};

const categoryLabels: Record<string, { titleKey: string; icon: React.ReactNode }> = {
  user: { titleKey: 'plugins_page.cat_user', icon: <LayoutGrid className="w-3 h-3 mr-1" /> },
  system: { titleKey: 'plugins_page.cat_system', icon: <Zap className="w-3 h-3 mr-1" /> },
  system_builtin: { titleKey: 'plugins_page.cat_system_builtin', icon: <Globe className="w-3 h-3 mr-1" /> }
};

const PluginsList: React.FC = () => {
  const { t } = useTranslation();
  const { user } = useAuthStore();
  const { settings } = useSettingsStore();
  const adminPath = settings?.site?.admin_path || 'admin1688';
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [loading, setLoading] = useState(false);
  const navigate = useNavigate();

  useEffect(() => {
    fetchPlugins();
  }, []);

  const sortPlugins = (list: Plugin[]): Plugin[] =>
    [...list].sort((a, b) => {
      const byEnabled = (b.is_enabled === 1 ? 1 : 0) - (a.is_enabled === 1 ? 1 : 0);
      if (byEnabled !== 0) return byEnabled;
      const byWeight = (b.admin_menu_sort ?? 0) - (a.admin_menu_sort ?? 0);
      if (byWeight !== 0) return byWeight;
      return (a.id ?? 0) - (b.id ?? 0);
    });

  const fetchPlugins = async () => {
    try {
      setLoading(true);
      const res = await (request.get('/plugins', { skipErrorHandler: true } as any) as any);
      if (res.plugins) setPlugins(sortPlugins(res.plugins));
    } catch {
      setPlugins([]);
    } finally {
      setLoading(false);
    }
  };

  const handleToggle = async (checked: boolean, plugin: Plugin, e: React.MouseEvent) => {
    e.stopPropagation();
    const nextEnabled = checked ? 1 : 0;
    try {
      await request.post(`/plugins/${plugin.name}/toggle`, { is_enabled: nextEnabled });
      setPlugins((prev) =>
        prev.map((p) => (p.name === plugin.name ? { ...p, is_enabled: nextEnabled } : p))
      );
      invalidateActivePluginsCache();
      invalidateAdminPluginsCache();
      message.success(checked ? t('plugins_page.enabled') : t('plugins_page.disabled'));
    } catch (error) {
      console.error(error);
    }
  };

  const isSuperAdmin = user?.role === 'admin' && !user?.admin_group_id;
  const hasGlobalPluginPerm = isSuperAdmin || user?.permissions?.includes('plugins');

  const hasPluginPerm = (pluginName: string) => {
    if (hasGlobalPluginPerm) return true;
    return user?.permissions?.includes(`plugin:${pluginName}`);
  };

  const visiblePlugins = plugins.filter(p => hasPluginPerm(p.name));

  const groupedPlugins: Record<string, Plugin[]> = {};
  visiblePlugins.forEach((p) => {
    const cat = p.category || 'user';
    if (!groupedPlugins[cat]) groupedPlugins[cat] = [];
    groupedPlugins[cat].push(p);
  });

  const categoryOrder = ['user', 'system', 'system_builtin'];

  const renderPluginCard = (plugin: Plugin) => {
    const isEnabled = plugin.is_enabled === 1;
    const cat = plugin.category || 'user';
    const style = categoryStyles[cat] || categoryStyles.user;
    const allowed = plugin.allowed_levels || 'all';
    const desc = t(`${plugin.name}:subtitle`, plugin.description || t('plugins_page.no_description'));
    const locales = pluginLocales[plugin.name] ? Object.keys(pluginLocales[plugin.name]) : [];

    return (
      <div
        key={plugin.id}
        onClick={() => navigate(`/${adminPath}/plugins/${plugin.name}/config`)}
        className={`group relative flex flex-col justify-between p-2.5 bg-card text-card-foreground rounded-lg border border-border cursor-pointer shadow-xs transition-all duration-200 hover:shadow-sm hover:bg-dashboard dark:hover:bg-muted ${style.cardBorder} ${!isEnabled ? 'opacity-75 hover:opacity-100' : ''}`}
      >
        <div>
          {/* 头部信息 */}
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2 min-w-0">
              <div className={`w-6 h-6 rounded flex items-center justify-center shrink-0 ${style.iconContainer} transition-transform duration-200 group-hover:scale-105`}>
                {pluginIcons[plugin.name] || <LayoutGrid className="w-3.5 h-3.5" />}
              </div>
              <div className="flex flex-col min-w-0">
                <h3 className="font-semibold text-[11.5px] text-foreground leading-tight truncate">
                  {t(`plugin_titles.${plugin.name}`, plugin.title)}
                </h3>
                <span className="text-[9px] text-muted-foreground/70 font-mono leading-tight truncate">
                  {plugin.name}
                </span>
              </div>
            </div>
            
            {/* 开关 */}
            <div onClick={(e) => e.stopPropagation()} className="flex items-center shrink-0">
              <Switch
                size="small"
                checked={isEnabled}
                onChange={(checked, e) => handleToggle(checked, plugin, e as any)}
              />
            </div>
          </div>

          {/* 插件简介（单行紧凑展示，悬停查看完整内容） */}
          <p
            title={desc}
            className="text-[10px] text-muted-foreground line-clamp-1 mt-1 leading-snug"
          >
            {desc}
          </p>
        </div>

        {/* 底部信息 & 配置按钮 */}
        <div className="mt-2 pt-1.5 border-t border-border/70 flex items-center justify-between gap-1.5">
          <div className="flex items-center gap-1 min-w-0 overflow-hidden">
            <span className="text-[9px] text-muted-foreground shrink-0">{t('plugins_page.open_to')}</span>
            <div className="flex items-center gap-1 overflow-hidden truncate">
              {allowed === 'all' ? (
                <span className={`px-1 py-0.5 text-[8.5px] font-medium rounded shrink-0 ${style.badge}`}>
                  {(cat === 'system' || cat === 'system_builtin') ? t('plugins_page.all_admins') : t('plugins_page.all_levels')}
                </span>
              ) : (
                allowed.split(',').slice(0, 2).map(lv => (
                  <span 
                    key={lv} 
                    className="px-1 py-0.5 text-[8.5px] font-medium bg-muted text-muted-foreground border border-border rounded truncate"
                  >
                    {lv}
                  </span>
                ))
              )}
            </div>
            {locales.length > 0 && (
              <span
                title={`${t('plugins_page.languages')} ${locales.map(lng => langNames[lng] || lng).join(' / ')}`}
                className="inline-flex items-center gap-0.5 px-1 py-0.5 text-[8px] font-medium bg-emerald-50 dark:bg-emerald-950/20 text-emerald-600 dark:text-emerald-400 border border-emerald-100/50 dark:border-emerald-900/20 rounded shrink-0"
              >
                <Globe className="w-2.5 h-2.5" />
                {locales.length}
              </span>
            )}
          </div>

          {/* 配置按钮 */}
          <button
            onClick={(e) => {
              e.stopPropagation();
              navigate(`/${adminPath}/plugins/${plugin.name}/config`);
            }}
            className="flex items-center gap-1 px-1.5 py-0.5 rounded border border-border text-[9px] font-medium text-muted-foreground hover:text-foreground bg-muted/30 hover:bg-muted transition-colors cursor-pointer shrink-0"
          >
            <Settings className="w-2.5 h-2.5" />
            {t('plugins_page.configure')}
          </button>
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-3">
      {/* 页头 */}
      <div className="flex items-center justify-between pb-2 mb-2 border-b border-border">
        <div className="flex flex-col gap-0.5">
          <div className="text-[13px] font-bold text-foreground leading-tight">{t('menu.plugins')}</div>
          <div className="text-[10.5px] text-muted-foreground leading-normal">{t('plugins_page.subtitle')}</div>
        </div>
      </div>

      {loading && plugins.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-16 gap-2">
          <Spin size="small" />
          <span className="text-[11px] text-muted-foreground">{t('plugins_page.loading')}</span>
        </div>
      ) : plugins.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-20 gap-3 text-center">
          <div className="w-12 h-12 rounded-full bg-muted/50 flex items-center justify-center mb-2">
            <LayoutGrid className="w-6 h-6 text-muted-foreground/50" />
          </div>
          <p className="text-[13px] font-medium text-foreground">{t('plugins_page.empty_title')}</p>
          <p className="text-[11px] text-muted-foreground max-w-[250px]">
            {t('plugins_page.empty_desc')}
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          {categoryOrder
            .filter((cat) => groupedPlugins[cat]?.length)
            .map((cat) => {
              const label = categoryLabels[cat] || categoryLabels.user;
              return (
                <div key={cat} className="space-y-2">
                  <div className="flex items-center">
                    <div className={`flex items-center text-[11px] font-semibold uppercase tracking-wider ${
                      cat === 'system' 
                        ? 'text-amber-600 dark:text-amber-400' 
                        : cat === 'system_builtin' 
                        ? 'text-emerald-600 dark:text-emerald-400' 
                        : 'text-zinc-500 dark:text-zinc-400'
                    }`}>
                      {label.icon}
                      {t(label.titleKey)}
                      <span className="ml-1 text-[9px] text-muted-foreground lowercase font-normal">
                        ({groupedPlugins[cat].length})
                      </span>
                    </div>
                    <div className="ml-2.5 flex-1 h-[1px] bg-border" />
                  </div>
                  <div className="grid gap-2.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))' }}>
                    {groupedPlugins[cat].map(renderPluginCard)}
                  </div>
                </div>
              );
            })}
        </div>
      )}
    </div>
  );
};

export default PluginsList;

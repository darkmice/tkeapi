/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { lazy } from 'react';

// 仅精确按需扫描插件入口文件，禁止递归 /**/*.tsx 导致 600+ 内部细碎组件被碎片化拆包
const pluginComponents = import.meta.glob([
  './pages/Plugins/*.tsx',
  './pages/Plugins/*/*.tsx',
  './pages/Plugins/*/components/*.tsx',
]);

const missingModule = () =>
  React.createElement(
    'div',
    { style: { padding: '20px', textAlign: 'center', color: '#666' } },
    'Plugin module is not installed or has been removed.'
  );

const loadPluginComponent = (path: string) => {
  const importFn = pluginComponents[`./pages/Plugins/${path}`];
  if (importFn) {
    return lazy(importFn as any);
  }
  return missingModule;
};

const silentMissingModule = () => null;

const loadOptionalPluginComponent = (path: string) => {
  const importFn = pluginComponents[`./pages/Plugins/${path}`];
  if (importFn) {
    return lazy(async () => {
      try {
        return (await (importFn as any)()) as { default: React.ComponentType<any> };
      } catch (err) {
        console.warn(`[plugins-registry] Failed to load optional plugin component (${path}):`, err);
        return { default: silentMissingModule };
      }
    });
  }
  return silentMissingModule;
};

export const RelayAPI = loadPluginComponent('DocsApi/RelayAPI.tsx');
export const PluginsList = loadPluginComponent('PluginsList.tsx');
export const PluginConfig = loadPluginComponent('PluginConfig.tsx');
export const ArkUserDashboard = loadPluginComponent('VolcengineArkMonitor/ArkUserDashboard.tsx');

// PluginConfig internal dependencies
export const AdminPresetAssets = loadPluginComponent('AssetManager/AdminPresetAssets.tsx');
export const RelayConvertAssets = loadPluginComponent('AssetManager/RelayConvertAssets.tsx');
export const ApiProxyAssets = loadPluginComponent('AssetManager/ApiProxyAssets.tsx');
export const CloudAssetsTab = loadPluginComponent('AssetManager/CloudAssetsTab.tsx');
export const ApiAccessConfig = loadPluginComponent('AssetManager/ApiAccessConfig.tsx');

export const TeamConfig = loadPluginComponent('TeamMarketing/TeamConfig.tsx');
export const ThemePromo = loadPluginComponent('TeamMarketing/ThemePromo.tsx');
export const ThemePromoLanding = loadPluginComponent('TeamMarketing/ThemePromoLanding.tsx');
export const SiteIconsManager = loadPluginComponent('SiteIcons/SiteIconsManager.tsx');
export const PortalManager = loadPluginComponent('SitePortal/PortalManager.tsx');
export const PortalStyleSelection = loadPluginComponent('SitePortal/PortalStyleSelection.tsx');
export const PortalManagerPro = loadPluginComponent('SitePortalPro/PortalManager.tsx');
export const PortalArticleManagerPro = loadPluginComponent('SitePortalPro/PortalArticleManagerPro.tsx');
export const PortalStyleSelectionPro = loadPluginComponent('SitePortalPro/PortalStyleSelection.tsx');
export const PortalDocsViewer = loadPluginComponent('SitePortalPro/PortalDocsViewer.tsx');
export const PortalBlogViewer = loadPluginComponent('SitePortalPro/PortalBlogViewer.tsx');
export const PortalDocsManager = loadPluginComponent('SitePortalPro/PortalDocsManager.tsx');
export const DocsManager = loadPluginComponent('DocsApi/DocsManager.tsx');
export const HaLogs = loadPluginComponent('HighAvailability/HaLogs.tsx');

// User-facing plugin pages (all under pages/Plugins/)
export const Playground = loadPluginComponent('Playground/Playground.tsx');
export const PlaygroundHome = loadPluginComponent('Playground/PlaygroundHome.tsx');
export const Playground2026 = loadPluginComponent('Playground_2026/Playground.tsx');
export const PlaygroundHome2026 = loadPluginComponent('Playground_2026/PlaygroundHome.tsx');
export const WorkflowCreateBootstrap2026 = loadPluginComponent(
  'Playground_2026/components/WorkflowCreateBootstrap.tsx',
);
export const ModelMarketplace = loadPluginComponent('ModelMarketplace/ModelMarketplace.tsx');
export const UserAssets = loadPluginComponent('UserAssets/UserAssets.tsx');
export const AdvancedMarketing = loadPluginComponent('AdvancedMarketing/AdvancedMarketing.tsx');
export const ReferralModelUsagePage = loadPluginComponent('AdvancedMarketing/ReferralModelUsagePage.tsx');
export const PlaygroundSkillConfigTab = loadPluginComponent(
  'Playground_2026/components/PlaygroundSkillConfigTab.tsx',
);
export const PlaygroundChatConfigTab = loadPluginComponent(
  'Playground_2026/components/PlaygroundChatConfigTab.tsx',
);
export const PlaygroundPromptOptimizeConfigTab = loadPluginComponent(
  'Playground_2026/components/PlaygroundPromptOptimizeConfigTab.tsx',
);
export const PlaygroundErrorMappingConfigTab = loadPluginComponent(
  'Playground_2026/components/PlaygroundErrorMappingConfigTab.tsx',
);
export const PlaygroundImageEditConfigTab = loadPluginComponent(
  'Playground_2026/components/PlaygroundImageEditConfigTab.tsx',
);
export const PlaygroundVideoEditConfigTab = loadPluginComponent(
  'Playground_2026/components/PlaygroundVideoEditConfigTab.tsx',
);
export const PlaygroundDemoImageConfig = loadPluginComponent(
  'Playground_2026/components/PlaygroundDemoImageConfig.tsx',
);
export const PlaygroundUserNavConfig = loadPluginComponent(
  'Playground_2026/components/PlaygroundUserNavConfig.tsx',
);
export const MarketingGamePopupHost = loadOptionalPluginComponent(
  'MarketingGames/MarketingGamePopupHost.tsx',
);
export const MarketingGameCampaignEdit = loadPluginComponent(
  'MarketingGames/CampaignEdit.tsx',
);
export const PlanEdit = loadPluginComponent('MarketingMembership/PlanEdit.tsx');
export const MarketingMembershipCampaignEdit = loadPluginComponent(
  'MarketingMembership/CampaignEdit.tsx',
);
export const MyTickets = loadPluginComponent('SupportTickets/MyTickets.tsx');

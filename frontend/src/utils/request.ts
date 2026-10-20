/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import axios from 'axios';
import { message } from 'antd';
import i18n from '../i18n';
import { resolveTimedisplay } from './timedisplay';
import { isAwaitingFreshSetup } from './freshSetup';

// 全局限制同时最多显示 3 条消息
message.config({ maxCount: 3 });

const bilingualNotice = (zh: string, en: string) => `${zh}\n${en}`;

const request = axios.create({
  baseURL: import.meta.env.VITE_API_BASE || '/api/v1',
  timeout: 300000, // 5 minutes timeout for long-running requests like image generation
});

/** 按当前路径决定 401 后的登录跳转目标；已在登录相关页则返回 null（不跳转） */
function resolveAuthRedirectPath(): string | null {
  const pathname = window.location.pathname;
  const adminPath = localStorage.getItem('tokensbyte_admin_path') || 'admin1688';
  const adminBase = `/${adminPath}`;
  const isAdminLogin = pathname === adminBase || pathname === `${adminBase}/`;
  const isUserAuthPage =
    pathname === '/login' ||
    pathname.startsWith('/login/') ||
    pathname === '/register' ||
    pathname === '/forgot-password';

  if (isAdminLogin || isUserAuthPage) return null;

  const isAdminArea = pathname === adminBase || pathname.startsWith(`${adminBase}/`);
  return isAdminArea ? adminBase : '/login';
}

request.interceptors.request.use(
  (config) => {
    const token = sessionStorage.getItem('token') || localStorage.getItem('token');
    // 若请求已显式携带 Authorization（如游乐场 API Key），则不覆盖
    if (token && !config.headers.Authorization && !config.headers.authorization) {
      config.headers.Authorization = `Bearer ${token}`;
    }
    try {
      // timedisplay：管理后台展示时区，用户端个人时区
      config.headers['x-timezone'] = resolveTimedisplay();
    } catch (e) {}
    return config;
  },
  (error) => {
    return Promise.reject(error);
  }
);

request.interceptors.response.use(
  (response) => {
    // Blob 响应（如 CSV 导出）直接返回原始数据，不解包
    if (response.config.responseType === 'blob') {
      return response.data;
    }
    return response.data;
  },
  (error) => {
    const { response, config } = error;
    // 主动取消的请求不弹全局错误（页面卸载 / 新查询替换旧查询）
    if (
      axios.isCancel(error) ||
      error?.code === 'ERR_CANCELED' ||
      error?.name === 'CanceledError' ||
      error?.message === 'canceled' ||
      error?.message === 'Request aborted'
    ) {
      return Promise.reject(error);
    }
    // 如果请求配置了 skipErrorHandler，或者特定 URL，则不弹出全局错误提示
    if ((config as any)?.skipErrorHandler || config?.url === '/plugins') {
      return Promise.reject(error);
    }
    if (response) {
      const { status, data } = response;

      let serverMsg =
        data?.error?.message ||
        data?.message ||
        (typeof data?.error === 'string' ? data.error : undefined) ||
        (typeof data === 'string' && !data.trimStart().startsWith('<') ? data : undefined);

      // 开发环境：代理无正文的 502/503/504 才当编译重启；有业务错误原文则照常展示
      if (
        import.meta.env.DEV &&
        !serverMsg &&
        (status === 502 || status === 503 || status === 504)
      ) {
        message.destroy('dev-backend-down');
        message.warning({
          content: bilingualNotice('后端服务正在编译或重启中...', 'Backend is compiling or restarting...'),
          key: 'dev-backend-down',
          duration: 3,
        });
        return Promise.reject(error);
      }
      
      // Translate specific backend error messages
      if (serverMsg === 'Account disabled') {
        serverMsg = i18n.t('login.account_disabled');
      } else if (serverMsg === '无编辑权限') {
        serverMsg = i18n.t('admin_perm.no_edit');
      } else if (serverMsg === '仅超级管理员可执行此操作') {
        serverMsg = i18n.t('admin_perm.super_only');
      } else if (serverMsg === 'Invalid or already used redemption code') {
        serverMsg = i18n.language?.startsWith('zh')
          ? '兑换码无效或已被使用'
          : 'Invalid or already used redemption code';
      }

      // 🎯 拦截 429 频控：替换为亲和黄色提示，避免红叉报错与刷屏
      if (status === 429) {
        message.destroy('global-rate-limit-warning');
        message.warning({
          content: serverMsg || '操作过于频繁，请稍后再试',
          key: 'global-rate-limit-warning',
          duration: 3,
        });
        return Promise.reject(error);
      }

      if (status === 401) {
        // 清空数据库后正在等待全新安装：不要被残留 401 抢跳，打断重启等待
        if (isAwaitingFreshSetup()) {
          return Promise.reject(error);
        }
        // 区分"业务认证失败"与"登录态过期/缺失"：
        // - 管理端受保护页 → 跳管理登录页；用户端 → /login
        // - 已在登录页：展示业务错误；忽略退出后残留请求的 Authentication required
        const isImpersonating = !!sessionStorage.getItem('token');
        const hadToken = isImpersonating || !!localStorage.getItem('token');
        if (hadToken) {
          if (isImpersonating) {
            // 代理登录态过期，仅清 session，保留管理员的 localStorage
            sessionStorage.removeItem('token');
            sessionStorage.removeItem('user');
          } else {
            // 正常登录态过期，全部清除
            localStorage.removeItem('token');
            localStorage.removeItem('user');
          }
        }

        const redirectTo = resolveAuthRedirectPath();
        if (redirectTo) {
          if (hadToken) {
            message.error('登录状态已过期，请重新登录');
          }
          window.location.href = redirectTo;
        } else if (serverMsg && serverMsg !== 'Authentication required') {
          message.error(serverMsg);
        } else if (!serverMsg) {
          message.error('认证失败');
        }
      } else {
        message.error(serverMsg || 'Request failed');
      }
    } else {
      if (import.meta.env.DEV) {
        message.destroy('dev-backend-down');
        message.warning({
          content: bilingualNotice('后端服务离线或正在编译重启...', 'Backend is offline or compiling/restarting...'),
          key: 'dev-backend-down',
          duration: 3,
        });
      } else {
        message.error('Network error');
      }
    }
    return Promise.reject(error);
  }
);

export default request;

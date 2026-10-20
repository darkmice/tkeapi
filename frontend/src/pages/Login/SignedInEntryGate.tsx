/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useLayoutEffect } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';
import useAuthStore from '../../store/auth';
import { sanitizeRedirectPath } from '../../utils/safeRedirect';
import { playground2026LoginLanding } from '../../utils/siteDefaultLanding';

type EntryUser = {
  role?: string;
  user_group?: string;
  level_id?: string | number | null;
};

/** 管理员与显式 redirect 可同步离开；普通用户要等站点默认落地。 */
function syncAuthedEntryPath(user: EntryUser, explicitRedirect: string | null): string | null {
  if (user.role === 'admin') {
    const adminPath = localStorage.getItem('tokensbyte_admin_path') || 'admin1688';
    return `/${adminPath}/dashboard`;
  }
  if (explicitRedirect) return sanitizeRedirectPath(explicitRedirect);
  return null;
}

/**
 * 浏览器里已经有登录态时，不要先挂登录/注册壳（含加载圈）。
 * hold：先做完副作用（如团队邀请加入）再由调用方自己跳转，期间同样不展示登录壳。
 */
const SignedInEntryGate: React.FC<{
  enabled?: boolean;
  hold?: boolean;
  children: React.ReactNode;
}> = ({ enabled = true, hold = false, children }) => {
  const { token, user } = useAuthStore();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const signedIn = enabled && !!(token && user);
  const syncTarget = signedIn && user ? syncAuthedEntryPath(user, searchParams.get('redirect')) : null;

  useLayoutEffect(() => {
    if (!signedIn || syncTarget || !user) return;
    let cancelled = false;
    void playground2026LoginLanding(user).then((landing) => {
      if (!cancelled) navigate(landing || '/dashboard', { replace: true });
    });
    return () => {
      cancelled = true;
    };
  }, [signedIn, syncTarget, user, navigate]);

  if (hold) return null;
  if (!signedIn) return <>{children}</>;
  if (syncTarget) return <Navigate to={syncTarget} replace />;
  return null;
};

export default SignedInEntryGate;

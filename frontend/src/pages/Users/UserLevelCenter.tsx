/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState } from 'react';
import { Button, Card, Input, Space, Tabs, Typography, Grid } from 'antd';
import { PlusOutlined, CheckSquareOutlined } from '@ant-design/icons';
import { Navigate, useLocation, useNavigate, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import useAuthStore from '../../store/auth';
import useSettingsStore from '../../store/settings';
import { hasAdminChildMenuPermission } from '../../constants/adminMenuPermissions';
import UserLevels from './UserLevels';
import AdminGroups from './AdminGroups';

const { Title } = Typography;
const { useBreakpoint } = Grid;

const UserLevelCenter: React.FC = () => {
  const { t } = useTranslation();
  const screens = useBreakpoint();
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [userKeyword, setUserKeyword] = useState('');
  const [adminKeyword, setAdminKeyword] = useState('');
  const [isBatchEditMode, setIsBatchEditMode] = useState(false);
  const { settings } = useSettingsStore();
  const adminPath = settings?.site?.admin_path || 'admin1688';
  const user = useAuthStore((s) => s.user);
  const isSuperAdmin = user?.role === 'admin' && !user?.admin_group_id;
  const canUsers = hasAdminChildMenuPermission(user?.permissions, 'users', 'users.levels', isSuperAdmin);
  const canAdmins = hasAdminChildMenuPermission(user?.permissions, 'users', 'admin_groups', isSuperAdmin);

  if (location.pathname.endsWith('/admin-groups')) {
    return <Navigate to={`/${adminPath}/user-levels?tab=admins`} replace />;
  }

  const requested = searchParams.get('tab') === 'admins' ? 'admins' : 'users';
  let activeKey = requested;
  if (activeKey === 'admins' && !canAdmins && canUsers) activeKey = 'users';
  if (activeKey === 'users' && !canUsers && canAdmins) activeKey = 'admins';

  const isAdminTab = activeKey === 'admins';
  const keyword = isAdminTab ? adminKeyword : userKeyword;
  const setKeyword = isAdminTab ? setAdminKeyword : setUserKeyword;

  const items: { key: string; label: string; children: React.ReactNode }[] = [];
  if (canUsers) {
    items.push({
      key: 'users',
      label: t('user_levels.users_tab', '普通用户等级配置'),
      children: activeKey === 'users' ? (
        <UserLevels
          embedded
          keyword={userKeyword}
          isBatchEditMode={isBatchEditMode}
          onBatchEditModeChange={setIsBatchEditMode}
        />
      ) : null,
    });
  }
  if (canAdmins) {
    items.push({
      key: 'admins',
      label: t('user_levels.admins_tab', '管理员等级配置'),
      children: activeKey === 'admins' ? (
        <AdminGroups
          embedded
          keyword={adminKeyword}
          isBatchEditMode={isBatchEditMode}
          onBatchEditModeChange={setIsBatchEditMode}
        />
      ) : null,
    });
  }

  return (
    <Card bordered={false}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, marginBottom: 8, flexWrap: 'wrap' }}>
        <Title level={screens.xs ? 4 : 2} style={{ margin: 0 }}>
          {t('user_levels.title')}
        </Title>
        {items.length > 0 && (
          <Space wrap>
            <Input.Search
              placeholder="搜索等级名称 / 等级 ID"
              allowClear
              value={keyword}
              onChange={(e) => setKeyword(e.target.value)}
              onSearch={setKeyword}
              style={{ width: screens.xs ? '100%' : 220 }}
            />
            <Button
              icon={<CheckSquareOutlined />}
              type={isBatchEditMode ? 'primary' : 'default'}
              danger={isBatchEditMode}
              onClick={() => setIsBatchEditMode(!isBatchEditMode)}
            >
              {isBatchEditMode ? '退出选择' : '选择编辑'}
            </Button>
            <Button
              type="primary"
              icon={<PlusOutlined />}
              onClick={() => navigate(isAdminTab ? `/${adminPath}/admin-groups/new` : `/${adminPath}/user-levels/new`)}
            >
              {isAdminTab ? '添加管理员等级' : t('user_levels.add_level')}
            </Button>
          </Space>
        )}
      </div>
      <Tabs
        activeKey={items.some((item) => item.key === activeKey) ? activeKey : items[0]?.key}
        onChange={(key) => {
          setIsBatchEditMode(false);
          if (key === 'admins') setSearchParams({ tab: 'admins' });
          else setSearchParams({});
        }}
        items={items}
      />
    </Card>
  );
};

export default UserLevelCenter;

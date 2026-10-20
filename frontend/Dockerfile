ARG DOCKER_IMAGE_PREFIX=""
# Nginx Production Stage
FROM ${DOCKER_IMAGE_PREFIX}nginx:stable-alpine
# 静态资源以版本目录 + 符号链接组织：dist 烘焙在 /srv/web-baked，容器启动时播种为 /srv/web/releases/baked 并在 current 缺失时指向它；
# 热更新时 ./web 卷挂载到 /srv/web，由 hot-update.sh 原子切换 current
COPY dist /srv/web-baked
# 后端上游由 /etc/nginx/upstream/active.conf 声明：镜像内置默认指向 backend:3000，蓝绿热更新时由宿主机卷覆盖并 nginx -s reload 切换
RUN mkdir -p /etc/nginx/upstream \
    && echo 'upstream tb_backend { server backend:3000; }' > /etc/nginx/upstream/active.conf \
    && sed -i 's/worker_connections *[0-9]*;/worker_connections 65535;/' /etc/nginx/nginx.conf \
    && sed -i '1i worker_rlimit_nofile 65535;' /etc/nginx/nginx.conf
# Custom Nginx configuration for React router support
RUN echo 'include /etc/nginx/upstream/active.conf; \
    map $http_x_forwarded_proto $forwarded_proto { \
    default $http_x_forwarded_proto; \
    "" $scheme; \
    } \
    map $http_host $forwarded_host { \
    default $http_host; \
    "" $host; \
    } \
    map $http_x_real_ip $client_real_ip { \
    default $http_x_real_ip; \
    "" $remote_addr; \
    } \
    log_format frontend_log "$client_real_ip - $remote_user [$time_local] \"$request\" $status $body_bytes_sent \"$http_referer\" \"$http_user_agent\" \"$http_x_forwarded_for\""; \
    server { \
    listen 80; \
    set_real_ip_from 127.0.0.1; \
    set_real_ip_from 10.0.0.0/8; \
    set_real_ip_from 172.16.0.0/12; \
    set_real_ip_from 192.168.0.0/16; \
    real_ip_header X-Forwarded-For; \
    real_ip_recursive on; \
    client_max_body_size 512m; \
    client_body_buffer_size 2m; \
    client_body_timeout 60s; \
    send_timeout 120s; \
    proxy_request_buffering off; \
    proxy_next_upstream off; \
    access_log /var/log/nginx/access.log frontend_log buffer=64k flush=5s; \
    # /home/models、/models 由 React SPA 渲染（含查询参数刷新也回 index.html）
    location ^~ /home/models { \
    root /srv/web/current; \
    try_files $uri /index.html; \
    } \
    location ^~ /models { \
    root /srv/web/current; \
    try_files $uri /index.html; \
    } \
    location /home-pro/guide { \
    root /srv/web/current; \
    try_files $uri /index.html; \
    } \
    location /home-pro/docs { \
    root /srv/web/current; \
    try_files $uri /index.html; \
    } \
    location /home-pro/devdocs { \
    root /srv/web/current; \
    try_files $uri /index.html; \
    } \
    location /home-pro/blog { \
    root /srv/web/current; \
    try_files $uri /index.html; \
    } \
    location /blog { \
    root /srv/web/current; \
    try_files $uri /index.html; \
    } \
    location /devdocs { \
    proxy_pass http://tb_backend/portal-devdocs; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_intercept_errors on; \
    error_page 418 502 503 504 = @portal_spa; \
    } \
    # 其余门户路径代理到后端
    location /home { \
    proxy_pass http://tb_backend; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_connect_timeout 900s; \
    proxy_send_timeout 900s; \
    proxy_read_timeout 900s; \
    } \
    # 站点协议由后端直接输出静态 HTML 文件
    location /legal { \
    proxy_pass http://tb_backend; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_connect_timeout 900s; \
    proxy_send_timeout 900s; \
    proxy_read_timeout 900s; \
    } \
    # 门户增强版开启时，根路径、关于、联系由后端直接输出 HTML。插件关闭返回 418，回退控制台。
    location = / { \
    proxy_pass http://tb_backend/portal-root; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_intercept_errors on; \
    error_page 418 502 503 504 = @portal_spa; \
    } \
    location = /about { \
    proxy_pass http://tb_backend/portal-about; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_intercept_errors on; \
    error_page 418 502 503 504 = @portal_spa; \
    } \
    location = /contact { \
    proxy_pass http://tb_backend/portal-contact; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_intercept_errors on; \
    error_page 418 502 503 504 = @portal_spa; \
    } \
    # 门户其它语言的静态页：/en/、/en/about、/en/contact。未发布时同样回退控制台。
    location ~ ^/(zh|zh-TW|en|ja|ko|vi)(/|/about/?|/contact/?)?$ { \
    rewrite ^/(.*)$ /portal-lang/$1 break; \
    proxy_pass http://tb_backend; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_intercept_errors on; \
    error_page 418 502 503 504 = @portal_spa; \
    } \
    location = /sitemap.xml { \
    proxy_pass http://tb_backend/portal-sitemap.xml; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    } \
    location = /robots.txt { \
    proxy_pass http://tb_backend/portal-robots.txt; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    } \
    location = /portal-published { \
    proxy_pass http://tb_backend/portal-published; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    } \
    # 静态页的 css / js / images 在门户目录 assets 下，经 /portal-pro 读出
    location /portal-pro/ { \
    proxy_pass http://tb_backend; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_connect_timeout 900s; \
    proxy_send_timeout 900s; \
    proxy_read_timeout 900s; \
    } \
    location @portal_spa { \
    root /srv/web/current; \
    try_files /index.html =404; \
    } \
    # 热更新：入口页禁止缓存，切换版本后浏览器下一次请求即拿到新 index.html（带 hash 的静态资源不受影响）
    location = /index.html { \
    root /srv/web/current; \
    add_header Cache-Control "no-cache"; \
    } \
    # 其他路径保持原有前端SPA逻辑
    location / { \
    root /srv/web/current; \
    index index.html index.htm; \
    try_files $uri $uri/ /index.html; \
    } \
    location /assets/icons/ { \
    proxy_pass http://tb_backend; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_connect_timeout 900s; \
    proxy_send_timeout 900s; \
    proxy_read_timeout 900s; \
    } \
    location /api { \
    proxy_pass http://tb_backend; \
    proxy_set_header Host $forwarded_host; \
    proxy_set_header X-Real-IP $client_real_ip; \
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
    proxy_set_header X-Forwarded-Proto $forwarded_proto; \
    proxy_set_header X-Forwarded-Host $forwarded_host; \
    proxy_buffering off; \
    proxy_cache off; \
    chunked_transfer_encoding on; \
    proxy_connect_timeout 900s; \
    proxy_send_timeout 900s; \
    proxy_read_timeout 900s; \
    } \
    location /v1 { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /v2 { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /compatible-mode { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /compatible-api { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /anthropic { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /v1beta { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /text-to-video { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /image-to-video { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /omni-video { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /tasks { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
    location /ent { \
        proxy_pass http://tb_backend; \
        proxy_set_header Host $forwarded_host; \
        proxy_set_header X-Real-IP $client_real_ip; \
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for; \
        proxy_set_header X-Forwarded-Proto $forwarded_proto; \
        proxy_buffering off; \
        proxy_cache off; \
        chunked_transfer_encoding on; \
        proxy_connect_timeout 900s; \
        proxy_send_timeout 900s; \
        proxy_read_timeout 900s; \
    } \
}' > /etc/nginx/conf.d/default.conf

EXPOSE 80
# 启动前播种：空卷或首次挂载时补齐 baked 版本、current 链接与默认上游声明，保证旧部署方式（无脚本）也能直接启动
CMD ["sh", "-c", "mkdir -p /srv/web/releases && rm -rf /srv/web/releases/baked && cp -a /srv/web-baked /srv/web/releases/baked && { [ -e /srv/web/current ] || ln -s releases/baked /srv/web/current; } && { [ -s /etc/nginx/upstream/active.conf ] || echo 'upstream tb_backend { server backend:3000; }' > /etc/nginx/upstream/active.conf; } && sed -i \"s/client_max_body_size [0-9]*[kmgMGTK]*;/client_max_body_size ${CLIENT_MAX_BODY_SIZE:-50m};/\" /etc/nginx/conf.d/default.conf && nginx -g 'daemon off;'"]

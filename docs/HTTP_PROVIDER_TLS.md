# HTTP Provider TLS Policy

`cx` supports an optional HTTP adapter path (`CX_PROVIDER_ADAPTER=http-curl`).

## Runtime policy (in-process)

- `CX_HTTP_REQUIRE_HTTPS=1` (default): blocks non-HTTPS provider URLs.
- `CX_HTTP_ALLOW_LOCAL_HTTP=1` (default): allows plain HTTP only for loopback hosts:
  - `http://localhost`
  - `http://127.0.0.1`
  - `http://[::1]`
- `CX_HTTP_ALLOWED_HOSTS` (optional CSV): host allowlist gate for `CX_HTTP_PROVIDER_URL`.
- `CX_HTTP_TLS_PINNEDPUBKEY` (optional): passed to curl `--pinnedpubkey` for TLS pinning.

Behavior:
- `https://...` always allowed.
- `http://...` non-loopback is rejected by default.
- Set `CX_HTTP_REQUIRE_HTTPS=0` only for controlled local testing.

## Deployment pattern (out-of-process TLS termination)

Use TLS at the edge and point CX to the HTTPS endpoint.

### Caddy example

```caddyfile
llm.example.com {
  reverse_proxy 127.0.0.1:8081
}
```

Set:

```bash
export CX_PROVIDER_ADAPTER=http-curl
export CX_HTTP_PROVIDER_URL=https://llm.example.com/v1/chat
export CX_HTTP_REQUIRE_HTTPS=1
```

### Nginx example

```nginx
server {
  listen 443 ssl;
  server_name llm.example.com;

  ssl_certificate /etc/letsencrypt/live/llm.example.com/fullchain.pem;
  ssl_certificate_key /etc/letsencrypt/live/llm.example.com/privkey.pem;

  location / {
    proxy_pass http://127.0.0.1:8081;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Proto https;
  }
}
```

## Operator checks

```bash
./bin/cx version | rg 'provider_transport|http_require_https|http_allow_local_http'
./bin/cx core | rg 'provider_transport|http_require_https|http_allow_local_http'
./bin/cx version | rg 'http_allowed_hosts|http_tls_pinning'
./bin/cx core | rg 'http_allowed_hosts|http_tls_pinning'
```

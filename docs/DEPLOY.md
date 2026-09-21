On the web: https://weechatradio.com/docs/deploy.html

# Deploy on DigitalOcean

## What you need

- A DigitalOcean account
- Domain `weechatradio.com` (already owned)
- A 2 GB Ubuntu LTS droplet
- A GitHub repo with Actions secrets: `DROPLET_HOST`, `DROPLET_USER`, `DROPLET_SSH_KEY`

## DNS

Create A records, all pointing at the droplet IP:

- `weechatradio.com`
- `www.weechatradio.com`
- `hub.weechatradio.com`
- `api.weechatradio.com`

## Steps

1. Create a 2 GB Ubuntu droplet in a region close to you. Allow ports 22, 80, 443.

2. Copy `deploy/` to the droplet (or clone this repo).

3. Copy `deploy/.env.example` to `/opt/wcr/.env` and set `WCR_DOMAIN=weechatradio.com`. For Email (Resend), add `RESEND_API_KEY` from your Resend dashboard (WeeChat Radio key). **Do not** put the key in git. Resend DNS (MX/SPF/DKIM) belongs on **`mail.weechatradio.com`** only — do not move apex `weechatradio.com` MX away from your existing mail/site setup. Webhook: `https://hub.weechatradio.com/api/v1/mail/webhook/resend` for `email.received`.

4. Point cloud-init at `deploy/cloud-init.yaml` on first boot, **or** run:

   ```
   cd /opt/wcr
   docker compose -f deploy/docker-compose.yml up -d
   ```

5. Caddy issues certificates for the three names. Wait a minute.

6. GitHub Actions workflow `.github/workflows/deploy.yml` rebuilds images and `docker compose pull && up -d` on each push to `main`.

## App version vs website

The website and hub **image** update on every push to `main`. That does not bump the app version and does not make `wcr update` download anything.

Bump `Cargo.toml`, the README badge, OpenAPI `info.version`, the numbered changelog, and a `v*` tag **only** when `wcr` / `wcr-gui` behaviour (or the bits inside the binary) change. `wcr update` follows those tags.

## Backups

Production uses **SQLite** inside the `hub` container (`/data/hub.db` and telemetry DB). Optional nightly copy:

```
docker compose -f deploy/docker-compose.yml exec -T hub \
  sh -c 'tar -czf - /data/*.db' > /var/backups/wcr-sqlite-$(date +%F).tar.gz
```

Copy the archive off the droplet (DO Spaces, `scp`, etc.). To restore, stop the stack, replace `/data/*.db` in the `wcr-data` volume, and `docker compose up -d`.

## How you know it worked

- `https://weechatradio.com` shows the map
- `https://hub.weechatradio.com/healthz` prints `ok`
- `wcr node` on a laptop connects to `wss://hub.weechatradio.com/ws`

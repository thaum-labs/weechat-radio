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

3. Copy `deploy/.env.example` to `/opt/wcr/.env` and set `WCR_DOMAIN=weechatradio.com`.

4. Point cloud-init at `deploy/cloud-init.yaml` on first boot, **or** run:

   ```
   cd /opt/wcr
   docker compose -f deploy/docker-compose.yml up -d
   ```

5. Caddy issues certificates for the three names. Wait a minute.

6. GitHub Actions workflow `.github/workflows/deploy.yml` rebuilds images and `docker compose pull && up -d` on each push to `main`.

## Backups

Optional nightly:

```
docker compose exec postgres pg_dump -U wcr wcr | gzip > /var/backups/wcr-$(date +%F).sql.gz
```

v1 can also run SQLite only (no Postgres) by setting `DATABASE_PATH=/data/wcr.db` and dropping the postgres service.

## How you know it worked

- `https://weechatradio.com` shows the map
- `https://hub.weechatradio.com/healthz` prints `ok`
- `wcr node` on a laptop connects to `wss://hub.weechatradio.com`

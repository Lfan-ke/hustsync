# Deployment

```mermaid
flowchart LR
    user["browser"] --> nginx
    nginx -- "/status.json" --> manager["manager :14242"]
    nginx -- "other pages" --> web["hust-mirrors"]
    worker["worker :6000"] -- "register, report status" --> manager
    manager -- "start, stop and other commands" --> worker
    ctl["hustsynctl"] --> manager
    prom["Prometheus"] -- "/metrics" --> manager
    prom -- "/metrics" --> worker
%%  browser -> nginx -+- /status.json -> manager :14242 <- hustsynctl
%%                    +- other pages --> hust-mirrors
%%  worker :6000 --register, report status--> manager
%%  manager --start, stop and other commands--> worker
%%  Prometheus --/metrics--> manager, worker
```

## systemd

`deploy/systemd/` has unit files for the manager and the worker: the binary goes in `/usr/bin/hustsync`, configs in `/etc/hustsync/`, and both run as the `hustsync` user.

```bash
useradd --system hustsync
install -m 755 target/release/hustsync target/release/hustsynctl /usr/bin/
install -m 644 deploy/systemd/hustsync-*.service /etc/systemd/system/
systemctl enable --now hustsync-manager hustsync-worker
```

`systemctl reload hustsync-worker` rereads the mirror list, as `hustsynctl reload -w <worker>` does. With cgroups, add `Delegate=yes` to the worker unit; see [cgroup.md](cgroup.md).

## docker compose

`deploy/compose/` brings up the whole set: manager, worker, the mirror site front end and nginx.

```bash
cd deploy/compose
docker compose up -d --build
```

Open `http://<host>:8080/syncing-status/` to see the mirrors in `worker.toml`. After editing `worker.toml`, run `docker compose restart worker`. Mirror data lives in the `data` volume and nginx serves it directly; the manager database lives in the `db` volume. Change the port with `HTTP_PORT=18080 docker compose up -d`.

For a worker that runs container mirrors, mount the host's `/var/run/docker.sock` into it and keep mirror directories at the same path on the host and in the worker, so containers mount the right directories.

`web` is built from source fetched from GitHub. If git cannot reach it, download the source archive, unpack it locally and point `build.context` at it in an override file.

## Front end

The mirror site front end, [hust-mirrors](https://github.com/hust-open-atom-club/hust-mirrors), reads status from `/status.json`, which nginx forwards to the manager's `/jobs`:

```nginx
location = /status.json {
    proxy_pass http://127.0.0.1:14242/jobs;
}
```

A mirror that has never synced reports `last_update` as `0001-01-01`, shown as N/A, as with tunasync.

## Monitoring

The manager and the worker serve `/metrics` and per-group `/metrics/<group>`, and can push to Telegraf, InfluxDB or Alloy; see [metrics.md](metrics.md) for metrics, config and receivers, and [monitoring.md](monitoring.md) for the full Prometheus, Grafana, Loki, Alloy and Telegraf stack with boards.

```yaml
scrape_configs:
  - job_name: hustsync
    static_configs:
      - targets: ["manager:14242", "worker:6000"]
```

Alert examples:

```yaml
- alert: MirrorStale
  expr: time() - hustsync_mirror_last_success_timestamp_seconds > 2 * 86400
- alert: WorkerDown
  expr: hustsync_worker_up == 0
  for: 10m
```

## Keeping tunasync scripts

`hustsynctl` takes the same arguments and prints the same fields as `tunasynctl`; arguments can go before or after the subcommand. For scripts that call `tunasynctl`, a link is enough:

```bash
ln -s /usr/bin/hustsynctl /usr/bin/tunasynctl
```

The compose image already has this link.

## Migrating from tunasync

| Item | What to do |
|:--:|:--:|
| Manager | Switch to the hustsync manager and set `db_type` to `redb`; bolt data is not migrated, and status fills in at the workers' next reports |
| Worker config | Use it as it is, including `zfs` and `btrfs_snapshot`; see [snapshot.md](snapshot.md) |
| Sync scripts | `TUNASYNC_*` variables are still provided |
| Containers | The name prefix changes from `tunasync-job-` to `hustsync-job-`; scripts that clean up containers by name need updating |
| Ports | The worker's `listen_port` now also serves `/metrics` |

During migration, run hustsync alongside tunasync first: hustsync syncs into another directory without serving it, and you compare both `/jobs` for a while before switching.

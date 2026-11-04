# Configuration

The manager and the worker each read a TOML file. Keys match tunasync, so most tunasync configs work as they are; the differences are listed at the end.

## manager

| Key | Default | Description |
|:--:|:--:|:--:|
| `debug` | none, required | Debug logs and request tracing |
| `server.addr` | none, required | Listen address |
| `server.port` | none, required | Listen port; tunasync uses 14242 |
| `server.ssl_cert` `server.ssl_key` | none, required | HTTPS when both are set |
| `files.db_type` | none, required | `redb` or `redis` |
| `files.db_file` | none, required | The redb file, or a redis URL such as `redis://host:6379/0` |
| `files.ca_cert` | none, required | CA trusted for HTTPS workers; empty uses the system roots |
| `files.status_file` | empty | Rewritten with the content of `/jobs` on every status change |

## worker

### global

| Key | Default | Description |
|:--:|:--:|:--:|
| `name` | empty | Worker name; the manager tells workers apart by it |
| `log_dir` | `/tmp/hustsync/log/{{.Name}}` | Log directory; `{{.Name}}` becomes the mirror name |
| `log_retention` | 10 | Log files kept per mirror, this run included |
| `log_gzip` | `false` | Compresses the kept older logs to `.gz` at rotation |
| `mirror_dir` | `/tmp/hustsync` | Root for mirror data; a mirror lives in `mirror_dir/mirror_subdir/<mirror>` |
| `concurrent` | 10 | Mirrors syncing at once |
| `interval` | 120 | Minutes between syncs |
| `retry` | 2 | Attempts per sync |
| `timeout` | 0 | Seconds per attempt; 0 means no limit |
| `exec_on_success` `exec_on_failure` | none | Commands run on the host after a sync succeeds or fails |
| `rsync_options` | none | Extra arguments for every rsync mirror |
| `dangerous_global_success_exit_codes` | none | Exit codes treated as success for every mirror |
| `dangerous_global_rsync_success_exit_codes` | none | Exit codes treated as success for every rsync mirror |

### manager

| Key | Default | Description |
|:--:|:--:|:--:|
| `api_base` | `http://localhost:14242` | Manager address |
| `api_base_list` | none | Several managers; status goes to each, and history is read from the first at start-up |
| `token` | empty | Sent when registering |
| `ca_cert` | empty | CA trusted for an HTTPS manager |
| `heartbeat` | none | Seconds between re-registrations; see [alert.md](alert.md) |

### server

| Key | Default | Description |
|:--:|:--:|:--:|
| `hostname` | `localhost` | Host name the manager uses to reach the worker |
| `listen_addr` | `127.0.0.1` | Control port address |
| `listen_port` | 6000 | Control port; also serves `/metrics` |
| `ssl_cert` `ssl_key` | empty | HTTPS when both are set |

### docker

| Key | Default | Description |
|:--:|:--:|:--:|
| `enable` | false | Mirrors with `docker_image` sync in a container |
| `volumes` | none | Volumes for every container, `host path:container path[:ro]` |
| `options` | none | Extra arguments for every `docker run` |

See [docker.md](docker.md).

### cgroup

| Key | Default | Description |
|:--:|:--:|:--:|
| `enable` | false | Each mirror syncs in its own cgroup |
| `base_path` | `/sys/fs/cgroup` | cgroupfs mount point |
| `group` | empty | Parent group; empty means the worker's own group |

See [cgroup.md](cgroup.md).

### zfs

| Key | Default | Description |
|:--:|:--:|:--:|
| `enable` | false | Each mirror's working directory must be a ZFS dataset of its own |
| `zpool` | none | Pool or parent dataset for the mirrors' datasets; required when enabled |

### btrfs_snapshot

| Key | Default | Description |
|:--:|:--:|:--:|
| `enable` | false | Working directories are Btrfs subvolumes, snapshotted after each successful sync |
| `snapshot_path` | none | Snapshot directory, one snapshot per mirror, named after it |

Both are Linux only; see [snapshot.md](snapshot.md).

### include

| Key | Description |
|:--:|:--:|
| `include_mirrors` | A glob; the `[[mirrors]]` of every matching file are appended |

## Logging

The manager and the worker can add a `[log]` table for more sinks besides the terminal or the `--log-file` chosen on the command line, written by [serlog](https://github.com/LeoHeco/ServerLog).

| Key | Description |
|:--:|:--:|
| `filter` | Filter directives, such as `info,hustsync_worker=debug`; ignored when `RUST_LOG` is set |
| `sinks` | List of sinks with keys from the serlog spec, such as `file`, `loki` or `mail` |

```toml
[log]
filter = "info"
sinks = [
  { kind = "loki", url = "http://alloy:3100/loki/api/v1/push", labels = { site = "hust" } },
  { kind = "file", path = "/var/log/hustsync/worker.json", format = "json", max_size = "64MiB", keep = 7 },
]
```

## Alerts

The manager's `[alert]` sets how many failed rounds raise an alert, and the worker's `[manager] heartbeat` sets the heartbeat interval; see [alert.md](alert.md).

## Metrics

The manager and the worker can both add `[metrics]`: `cache` sets each group's cache time in seconds, and each `[[metrics.push]]` is one push target; see [metrics.md](metrics.md).

## Mirrors

Each `[[mirrors]]` entry is one mirror. Unset keys take the `[global]` value; mirrors can nest `[[mirrors.mirrors]]`, and children inherit what the parent sets.

### Common

| Key | Default | Description |
|:--:|:--:|:--:|
| `name` | required | Mirror name |
| `provider` | `rsync` | `rsync`, `two-stage-rsync`, `command`, `fpsync`, `lsyncd` or `ftpsync` |
| `upstream` | empty | Upstream address; rsync providers need a trailing `/` |
| `interval` `retry` `timeout` | from global | As in global |
| `cron` | none | Schedules by a cron expression such as `0 */6 * * *` instead of `interval`; local time |
| `log_retention` `log_gzip` | from global | As in global |
| `mirror_dir` | none | The mirror directory itself; `mirror_subdir` is ignored |
| `mirror_subdir` | none | The level between `mirror_dir` and the mirror name |
| `log_dir` | from global | As in global |
| `role` | `master` | `slave` reports the mirror as a secondary |
| `env` | none | Extra environment for the sync process |
| `exec_on_success` `exec_on_failure` | from global | Replace the global setting |
| `exec_on_success_extra` `exec_on_failure_extra` | none | Appended after the global setting |
| `success_exit_codes` | none | Extra exit codes treated as success |
| `memory_limit` | none | Memory cap, such as `256M`; applies only with Docker or cgroups |
| `snapshot_path` | `[btrfs_snapshot]` directory plus the mirror name | This mirror's snapshot path; not inherited by children |

The sync process sees `HUSTSYNC_MIRROR_NAME`, `HUSTSYNC_WORKING_DIR`, `HUSTSYNC_UPSTREAM_URL`, `HUSTSYNC_LOG_DIR` and `HUSTSYNC_LOG_FILE`, plus `TUNASYNC_*` with the same values.

### rsync and two-stage-rsync

| Key | Default | Description |
|:--:|:--:|:--:|
| `command` | `rsync` | rsync executable |
| `username` `password` | none | Written to `USER` and `RSYNC_PASSWORD` |
| `exclude_file` | none | File for `--exclude-from` |
| `rsync_options` | none | Extra arguments; for two-stage, the second stage only |
| `rsync_override` | none | Replaces the default arguments; rsync only |
| `rsync_override_only` | false | Uses `rsync_override` alone; rsync only |
| `rsync_timeout` | 120 | Seconds for `--timeout` |
| `rsync_no_timeout` | false | Omits `--timeout` |
| `use_ipv6` `use_ipv4` | false | Adds `-6` or `-4` |
| `rsync_success_exit_codes` | none | Extra rsync exit codes treated as success |
| `stage1_profile` | `debian` | Excludes for the first stage of two-stage: `debian` or `debian-oldstyle` |

### command

| Key | Description |
|:--:|:--:|
| `command` | Split by shell rules and run directly, without a shell |
| `fail_on_match` | A match of this regex in the log fails the run |
| `size_pattern` | Regex for the mirror size in the log; the first group of the last match |

### docker, fpsync, lsyncd, ftpsync

| Key | Description |
|:--:|:--:|
| `docker_image` | Container image; see [docker.md](docker.md) |
| `docker_volumes` `docker_options` | Appended after the `[docker]` settings |
| `fpsync_options` `lsyncd_options` | Passed to fpsync or lsyncd as written; see [tools.md](tools.md) |
| `ftpsync_conf` | Lines appended to the generated ftpsync config; see [ftpsync.md](ftpsync.md) |

## Differences from tunasync

| Item | tunasync | hustsync |
|:--:|:--:|:--:|
| Database | bolt, badger, leveldb, redis | redb, redis |
| Unknown keys | Ignored | Rejected, so a misspelt key cannot silently do nothing |
| Environment | `TUNASYNC_*` | `HUSTSYNC_*`, plus `TUNASYNC_*` with the same values |
| Docker container name | `tunasync-job-<mirror>` | `hustsync-job-<mirror>` |
| Over `memory_limit` | An ordinary failure | Reported as OOM |
| New providers | none | `fpsync`, `lsyncd`, `ftpsync` |
| Scheduling | `interval` only | `cron` as well |
| Log files kept | Fixed at 10 | Set by `log_retention` |
| Old log compression | none | `log_gzip` |
| ZFS check | Mount point only | Also checks that the mount is ZFS |
| Btrfs snapshot update | Deletes the old snapshot before creating the new one, leaving the published path empty in between | Swaps the new snapshot in with one `rename` |

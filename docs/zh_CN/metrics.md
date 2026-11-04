# 指标

manager 与 worker 都输出 p8s 格式的指标，可被 Prometheus 或 Telegraf 拉取。

## 编译开关

| feature | 内容 | 默认 |
|:--:|:--:|:--:|
| `p8s` | `GET /metrics`、`/metrics/all`、`/metrics/<组>` | 开 |

```bash
cargo build --release
cargo build --release --no-default-features
```

关掉时不编入指标代码。配置里写了 `[metrics]`，启动时报错。

## 接口

| 路径 | 内容 |
|:--:|:--:|
| `/metrics`、`/metrics/all` | 全部组 |
| `/metrics/<组>` | 一组；组名写错返回 404，正文列出可用的组 |

manager 在自己的端口上提供，worker 在控制端口 `listen_port` 上提供。各组互不重叠，变化快的组可以抓得勤一些：

```yaml
scrape_configs:
  - job_name: hustsync-mirrors
    scrape_interval: 15s
    metrics_path: /metrics/mirrors
    static_configs: [{ targets: ["manager:14242"] }]
  - job_name: hustsync-rest
    scrape_interval: 1m
    metrics_path: /metrics/workers
    static_configs: [{ targets: ["manager:14242"] }]
```

## manager

| 组 | 指标 | 含义 |
|:--:|:--:|:--:|
| `mirrors` | `hustsync_mirrors{status}` | 各状态的镜像数，求和即镜像总数 |
| | `hustsync_mirror_sync_duration_seconds{mirror,worker}` | 上次同步用时 |
| | `hustsync_mirror_last_success_timestamp_seconds{mirror,worker}` | 上次成功的时间 |
| `workers` | `hustsync_workers` | 已注册的 worker 数 |
| | `hustsync_workers_online` `hustsync_worker_up{worker}` | 抓取时探测，2 秒内有响应即在线 |
| `syncs` | `hustsync_syncs_total{mirror,worker,result}` | 成功、失败的同步次数 |
| `process` | `hustsync_build_info{version}` 与 `process_*` | 版本；Linux 上另有 CPU、内存、文件句柄、线程数 |

## worker

| 组 | 指标 | 含义 |
|:--:|:--:|:--:|
| `jobs` | `hustsync_worker_jobs` | 镜像数 |
| | `hustsync_worker_jobs_running` | 正在同步的镜像数 |
| | `hustsync_worker_jobs_queued` | 等待下次同步的镜像数 |
| | `hustsync_worker_concurrency` | 并发上限 |
| `process` | 同 manager | |

## 配置

manager 与 worker 写法相同：

```toml
[metrics]
cache = { mirrors = 10, workers = 30 }
```

| 键 | 默认 | 说明 |
|:--:|:--:|:--:|
| `cache` | 空 | 组名到秒数；没写的组每次请求都现算 |

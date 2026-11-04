# 监控栈

`deploy/` 下备好了监控所需的配置，compose 的 `monitoring` profile 一条命令起齐：

```bash
cd deploy/compose
docker compose --profile monitoring up -d
```

## 组件

| 组件 | 配置 | 作用 |
|:--:|:--:|:--:|
| Prometheus 3.15 | `deploy/prometheus/prometheus.yml` | 各组按各自的间隔抓取：镜像状态 15 秒，同步计数 30 秒，worker 与进程 1 分钟 |
| 告警规则 | `deploy/prometheus/rules.yml` | 见下表 |
| Telegraf 1.40 | `deploy/telegraf/telegraf.conf` | 拉取各组，也在 8186 端口收推送，写进 InfluxDB |
| InfluxDB 3 Core | compose | 只在 compose 网络内可达；单次查询最多读 432 个 Parquet 文件，约五天的数据，查更久要加 `--query-file-limit` |

## 告警规则

| 规则 | 条件 |
|:--:|:--:|
| `HustsyncManagerDown` | manager 抓取失败 5 分钟 |
| `HustsyncWorkerDown` | worker 不响应 10 分钟 |
| `HustsyncMirrorFailing` | 连续失败 3 轮 |
| `HustsyncMirrorStale` | 两天未同步成功，暂停与禁用的除外，持续 30 分钟 |
| `HustsyncMirrorOverdue` | 过了排期一小时仍未开始，持续 15 分钟 |
| `HustsyncMirrorStuck` | 同步持续超过一天 |

规则带单元测试：

```bash
promtool test rules deploy/prometheus/rules.test.yml
```

接 Alertmanager 时在 `prometheus.yml` 加 `alerting` 一节。程序自己发的告警邮件见 [alert.md](alert.md)。

## 不用 compose 时

| 组件 | 做法 |
|:--:|:--:|
| Prometheus | 照 `prometheus.yml` 改 `targets` |
| Telegraf | 改 `urls` 与输出 |

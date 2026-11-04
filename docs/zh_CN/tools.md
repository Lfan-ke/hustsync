# fpsync

fpsync 只接受本地目录作为源，方向由 `upstream` 决定：

| `upstream` | 方向 | 场景 |
|:--:|:--:|:--:|
| 本地绝对路径，如 NFS 挂载点 | 从 `upstream` 同步到镜像目录 | 上游已挂载到本地 |
| 远端地址，如 `rsync://edge/debian/` 或 `edge::debian` | 把镜像目录推送到 `upstream` | 主站向从站分发 |

先用 fpart 把目录切成多份，再并行跑多个 rsync，适合文件极多的大目录。与其他镜像一样按 `interval` 定时同步。

```toml
[[mirrors]]
name = "debian"
provider = "fpsync"
upstream = "/mnt/nfs/debian"
fpsync_options = ["-n", "8", "-f", "20000"]
```

实际执行 `fpsync -n 8 -f 20000 /mnt/nfs/debian/ <镜像目录>/`。`fpsync_options` 原样传入，常用 `-n` 并行数、`-f` 每份文件数、`-s` 每份字节数。

## 安装

```bash
apt install fpart
```

`command` 可指定可执行文件路径。

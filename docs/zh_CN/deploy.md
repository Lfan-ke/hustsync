# 部署

```mermaid
flowchart LR
    user["浏览器"] --> nginx
    nginx -- "/status.json" --> manager["manager :14242"]
    nginx -- "其余页面" --> web["hust-mirrors"]
    worker["worker :6000"] -- "注册、上报状态" --> manager
    manager -- "start、stop 等命令" --> worker
    ctl["hustsynctl"] --> manager
%%  浏览器 -> nginx -+- /status.json -> manager :14242 <- hustsynctl
%%                   +- 其余页面 ----> hust-mirrors
%%  worker :6000 --注册、上报状态--> manager
%%  manager --start、stop 等命令--> worker
```

## systemd

`deploy/systemd/` 下有 manager 与 worker 的单元文件，二进制放 `/usr/bin/hustsync`，配置放 `/etc/hustsync/`，以 `hustsync` 用户运行。

```bash
useradd --system hustsync
install -m 755 target/release/hustsync target/release/hustsynctl /usr/bin/
install -m 644 deploy/systemd/hustsync-*.service /etc/systemd/system/
systemctl enable --now hustsync-manager hustsync-worker
```

`systemctl reload hustsync-worker` 重读镜像列表，等同 `hustsynctl reload -w <worker>`。

## docker compose

`deploy/compose/` 起一整套：manager、worker、镜像站前端与 nginx。

```bash
cd deploy/compose
docker compose up -d --build
```

浏览器打开 `http://<主机>:8080/syncing-status/` 即可看到 `worker.toml` 里的镜像。改 `worker.toml` 后 `docker compose restart worker`。镜像数据在卷 `data`，由 nginx 直接提供；manager 数据库在卷 `db`。端口用 `HTTP_PORT=18080 docker compose up -d` 修改。

`web` 从 GitHub 拉源码构建。git 连不上时下载源码包解压到本地，用覆盖文件把 `build.context` 指过去。

## 前端

镜像站前端 [hust-mirrors](https://github.com/hust-open-atom-club/hust-mirrors) 从 `/status.json` 读取状态，内容即 manager 的 `/jobs`，交给 nginx 转发：

```nginx
location = /status.json {
    proxy_pass http://127.0.0.1:14242/jobs;
}
```

从未同步成功的镜像，`last_update` 为 `0001-01-01`，前端显示为 N/A，与 tunasync 一致。

## 沿用 tunasync 的运维脚本

`hustsynctl` 的参数和输出字段与 `tunasynctl` 相同，参数既可放在子命令前，也可放在之后。脚本里写死 `tunasynctl` 的，建一个链接即可：

```bash
ln -s /usr/bin/hustsynctl /usr/bin/tunasynctl
```

compose 镜像里已经带了这个链接。

## 从 tunasync 迁移

| 项 | 做法 |
|:--:|:--:|
| manager | 换成 hustsync manager，`db_type` 改为 `redb`；bolt 数据不迁移，worker 下次上报后状态自动补齐 |
| worker 配置 | 原样使用；尚未支持的字段会报错，删掉即可 |
| 同步脚本 | `TUNASYNC_*` 环境变量照常提供 |

迁移时先让 hustsync 与 tunasync 并行跑：hustsync 同步到另一个目录、不对外服务，比对两边 `/jobs` 一段时间再切换。

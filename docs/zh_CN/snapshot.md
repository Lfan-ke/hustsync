# ZFS

只在 Linux 上可用，键名与 tunasync 相同，旧配置原样可用。

每个镜像的工作目录必须是一个独立的 ZFS 数据集。目录不存在、不是挂载点、或挂的不是 ZFS，这次同步都不执行，并在日志里给出建数据集的命令。

```toml
[zfs]
enable = true
zpool = "tank/mirrors"
```

`zpool` 是镜像数据集所在的池或父数据集。按上面的配置，镜像 `debian` 的数据集是 `tank/mirrors/debian`：

```sh
zfs create tank/mirrors/debian
zfs set mountpoint=/srv/mirror/debian tank/mirrors/debian
chown hustsync /srv/mirror/debian
```

hustsync 不建数据集，只检查。

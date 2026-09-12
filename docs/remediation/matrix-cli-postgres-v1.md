# Matrix v3：真实 CLI 进程与最小权限数据库回归

Status: proposed executable regression; execution acceptance remains evidence-bound  
Owner role: `matrix-integration`  
Production authorization: `not_granted`

## 目的和范围

现有 operator runner 按版本阶段应用迁移并执行 SQL 回归。本补充在完整 transport 0001–0005、
operator 0001–0006 之后，运行实际 `scripts/reconcile-matrix-adapter-result.py` 进程，
以新的仅继承 `cex_matrix_reconciler_runtime` 的 LOGIN 连接 PostgreSQL 16。
因此不再用手写 SQL 成功代替 CLI 入口、wrapper、参数构造和 libpq 调用的成功。

HTTP 对端是本机合成 lookup fixture，不是实际 Adapter、Consumer Entry 或 Matrix homeserver。
数据库、CLI 子进程、连接身份、v3 SQL、运行权限以及幂等状态变更是真实执行对象。
这仍不是生产托管、远程 TLS 握手、真实业务响应丢失、跨服务端到端或生产授权的证明。

## 安全前置条件

新增 runner 是 `scripts/test-matrix-cli-postgres.py`，默认不是 self-test，缺少数据库条件即失败。
必须同时设置 `MATRIX_TEST_ALLOW_SCHEMA_RESET=1` 和 `MATRIX_CLI_TEST_ALLOW_ROLE_CREATE=1`。
只接受 `MATRIX_TEST_DATABASE_URL` 中 literal `127.0.0.1` 和 exact database `matrix_review_ci`；
拒绝其他主机、localhost 名称、其他数据库、空凭证、URL query 和 fragment。
这两个开关只授权明确的可销毁 CI 数据库，绝不应配置到生产环境。

先验证 PostgreSQL 16 及三个历史/当前函数的完整存在。数据库所有者只负责测试 fixture 和临时 LOGIN；
LOGIN 是随机命名、随机口令的普通角色，明确禁止 superuser、createdb、createrole、replication、bypassrls。
测试通过真实新连接检查 current_user，不能用 owner 连接加一段表面上的角色文字替代。
子进程口令仅通过环境传递，不出现在命令行、正常报告或仓库文件。

实际 CLI 使用经过校验的 root-owned 普通 psql 文件，而不是 PATH 注入的 wrapper。
HTTP 仅监听随机 loopback 端口。故意向 CLI 父环境放入无效 PGSERVICE、PGSERVICEFILE、PGHOSTADDR、
PGUSER、PGDATABASE 和 PGOPTIONS，以验证实际生产入口对 libpq 环境的封闭构造。
退出时回收临时 LOGIN；不删除追加式证据、不 drop schema。残留测试行由可销毁数据库生命周期回收。
失败的权限探针在事务内执行并回滚，不能借测试去修改真实 main、生产数据库或现有角色权限。

## 用例与数据库断言

| 组 | 实际路径 | 必需结果 |
|---|---|---|
| 历史入口拒绝 | 真实普通 LOGIN 调用 v1 和 v2 | PostgreSQL SQLSTATE 42501，不把任意错误当权限证明 |
| 直接写拒绝 | 同一 LOGIN 尝试 UPDATE、DELETE、TRUNCATE | 均因权限拒绝；即使错误授予权限也不能提交变更 |
| 六个外层身份篡改 | delivery、payload hash、event、room、sender、request fingerprint | CLI 非零退出，只有一次 lookup，数据库原始快照不变 |
| 八个嵌入字段篡改 | schema、source marker、delivery、payload、event、room、principal、fingerprint | 同上，不可被正确外层字段掩盖 |
| 五个结构/任务反例 | 缺 raw、缺 invocation、invocation 不匹配、空 task/invocation、绑定额外字段 | 同上，不能把缺字段当兼容成功 |
| 首次恢复 | 实际 CLI → v3 → owner-only core | disposition=reconciled，原投递 dead_letter→sent |
| 新观察重放 | 再次运行实际 CLI，新的响应时间及响应摘要 | disposition=replay；总计一个终态转换、两个观察 |
| 终态内容碰撞 | 保留合法绑定但改变已接受结果内容 | 拒绝且完整数据库快照不变 |

成功报告含 26 个命名断言组，其中 DML 组包含三次权限探针；19 个坏响应是六加八加五。
这些是数据库模式执行完成后才可计入的断言，不与本地七个 self-test 相混。
同一 source event 仅有一个 adapter delivery，不新建 Matrix send；恢复 adapter 成功不是用户收到回复。
结果身份依赖持久化回执与完整绑定，不能仅凭一次 HTTP 200 或一个 task 字符串接受。

## 执行与门禁集成

在完整干净 checkout、已应用全部迁移的可销毁数据库中：

```text
python3 scripts/test-matrix-cli-postgres.py --self-test
python3 scripts/test-matrix-cli-postgres.py
```

第一条只执行 fixture/配置/HTTP/子进程边界测试，不连接 PostgreSQL。
第二条是实际数据库回归；缺客户端、数据库、迁移或显式授权不能降级为 skip/success。
不得为运行该命令自动安装到生产数据库或自动修改生产权限。

既有 `matrix-review-repair-regression` 保留所有原 jobs 和步骤；source-contracts 添加 self-test，
matrix-postgres 在原 transport 和 operator 回归之后运行实际 CLI 回归并保留 JSON 观察报告。
既有内容只读、pin、无持久 checkout 凭证和无 self-hosted 执行策略保持不变。
不创建新的辅助绿色 status 来取代原权威门禁。

报告绑定实际 source SHA/tree、被调用源文件 SHA-256、服务器版本和实际完成的断言组。
后续任何 source/workflow/merge 改变都按原规则重新资格化；报告不能证明后来的新源树。
产物必须结合相应 run attempt、真实 job/step/log 和原始摘要核验，文件名本身不算证据。

## 独立接受仍需补齐

维护者需要检视实际 lookup fixture 与当前 Adapter 响应契约是否一致；这是待审查的测试协议，
不能用自编 fixture 自动批准外部系统。真实响应丢失演练仍需启动 Adapter/Consumer/Relay，
证明业务提交后丢失响应、同一绑定可恢复、业务仅执行一次，且回复是否发送由自己的 send receipt 证明。
远程 PostgreSQL verify-full/channel-binding 的实际证书、服务身份和密钥托管也必须独立演练。

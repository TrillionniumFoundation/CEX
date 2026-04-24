# Configuration v1

## 当前新增配置化内容

目前已将 gateway-service 的关键运行参数抽离到环境变量：

- `GATEWAY_HOST`
- `GATEWAY_PORT`
- `LEDGER_BASE_URL`
- `EXECUTION_BASE_URL`

并通过 `crates/shared-config` 统一读取。

## 当前价值

这一步的意义不是“好看”，而是为后续这些工作清路：
- 服务地址切换
- 本地/测试/生产环境切换
- 未来容器化部署
- 数据库与 Redis 等配置统一管理

## 当前限制

- 目前只覆盖 gateway-service
- identity / ledger / execution / audit 还没有统一配置层
- 还没有 dotenv 或更完整配置体系

## 下一步建议

1. 扩展 shared-config 到其他服务
2. 增加数据库配置结构
3. 增加 Redis / NATS 配置结构
4. 统一启动参数与日志配置

# Ledger Fail-Fast Plan v1

## 1. 当前问题

ledger-service 当前处于“数据库优先尝试、内存回退”的过渡模式。这个模式适合早期接线，但不适合长期保留。

原因：
- 可能导致数据库和内存状态不一致
- 容易掩盖持久化错误
- 会让调用方误以为系统已经真正持久化

## 2. 建议的过渡阶段

### 阶段 A：当前阶段（已完成）
- 尝试写数据库
- 内存回退保底

### 阶段 B：受控切换阶段（下一步）
- 增加开关：`LEDGER_FAIL_FAST=true|false`
- 当开关为 true：repo 写失败直接返回错误
- 当开关为 false：允许过渡期回退

### 阶段 C：数据库主路径阶段
- create_account / append_entry 必须数据库成功
- get_account 优先数据库，内存只用于测试模式
- reserve / consume / refund 主要依赖数据库状态

### 阶段 D：移除内存保底
- 删除内存态账户和流水主路径
- 内存实现仅保留为测试/开发 stub

## 3. 推荐原则

对于账本系统，最终必须走向：
- 写失败即失败
- 持久化优先
- 幂等检查依赖持久化层

## 4. 下一步建议

1. 引入 `LEDGER_FAIL_FAST`
2. create_account 先做 fail-fast 试点
3. append_entry 后续也切换到 fail-fast

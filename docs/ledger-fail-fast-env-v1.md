# LEDGER_FAIL_FAST Env Plan v1

## 建议新增环境变量

```env
LEDGER_FAIL_FAST=false
```

## 语义

### `LEDGER_FAIL_FAST=false`
- 允许过渡期模式
- repository 失败时可保留内存回退
- 适合本地开发和数据库未完全接好时

### `LEDGER_FAIL_FAST=true`
- repository 写失败应直接返回错误
- 不应悄悄以内存成功掩盖数据库失败
- 适合切换到数据库主路径阶段

## 推荐推进方式

1. 先引入配置项
2. 先在 `create_account` 上试点
3. 再扩展到 `append_entry`
4. 最后扩展到 reserve / consume / refund 全路径

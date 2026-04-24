# ADR-001: 平台方向选择

- 状态：Accepted
- 日期：2026-04-07

## Context
团队希望以开源中心化交易所项目为基础，开发一个 AI 原生平台，并明确以 Rust 作为主要开发语言。

## Decision
1. 不直接继承完整开源 CEX 代码库。
2. 仅借鉴 CEX 的工程能力：账户、账本、风控、审计、API、高并发状态机。
3. 放弃传统交易所业务语义：symbol、orderbook、wallet deposit/withdraw、market data 等。
4. 使用 Rust 构建平台主干。
5. 将平台定位为 AI-native execution / governance / billing platform。

## Consequences
正面影响：降低历史包袱、语义更统一、与 AI-native 架构更一致、提升长期可维护性。  
代价：无法直接快速套用完整交易所产品，需要自行定义领域模型，前期架构设计要求更高。

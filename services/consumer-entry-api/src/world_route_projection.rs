use super::*;

fn escape_world_route_visible_text(value: &str) -> String {
    if let Some(copy) = i18n_span_from_bilingual_slash_copy(value) {
        return copy;
    }
    if contains_cjk_text(value) {
        let english = world_route_english_visible_text(value);
        return i18n_span_from_bilingual_slash_copy(&format!("{} / {}", english, value))
            .unwrap_or_else(|| escape_html_text(&english));
    }
    escape_html_text(value)
}

fn world_route_english_visible_text(value: &str) -> String {
    if !contains_cjk_text(value) {
        return value.to_string();
    }

    let replacements = [
        ("评级后升级悬赏", "post-rating bounty upgrade"),
        ("评级通过", "rating passed"),
        ("高阶范围", "upgraded scope"),
        ("赏金阶梯", "bounty ladder"),
        ("时间线", "timeline"),
        ("质量备注", "quality note"),
        ("成果证据", "result evidence"),
        ("委托方反馈", "client feedback"),
        ("世界状态变化", "world-state changes"),
        ("复盘", "review"),
        ("评级标准", "rating criteria"),
        ("缺失证据", "missing evidence"),
        ("异议", "objections"),
        ("委托目标", "commission goal"),
        ("里程碑", "milestone"),
        ("第一轮成果", "first result"),
        ("战果总结待生成", "Outcome summary pending"),
        ("结果摘要整理中", "Outcome summary pending"),
        ("路线摘要整理中", "Route summary pending"),
        ("证据和下一步整理中", "Evidence and next step pending"),
        ("支线提示待生成", "Branch hint pending"),
        ("支线打法待生成", "Branch playbook pending"),
        ("继续推进下一步机会", "continue the next opportunity"),
        ("跟进已完成任务", "follow up completed task"),
        ("跟进战报", "follow up battle report"),
        ("完成委托方评级", "complete client rating"),
        ("提交第一轮成果", "submit first result"),
        ("重新提交成果", "resubmit result"),
        ("打开任务牌路线", "Open bounty route"),
        ("打开契约路线", "Open contract route"),
        ("起草任务后续", "Draft task follow-up"),
        ("起草后续行动", "Draft next action"),
        ("起草后续支线", "Draft next branch"),
        ("推进下一条支线", "Advance next branch"),
        ("下一条支线", "next branch"),
        ("下一次协作", "next collaboration"),
        ("下一步", "next step"),
        ("支线", "branch"),
        ("事件", "events"),
        ("委托", "commissions"),
        ("契约", "contracts"),
        ("战报", "battle reports"),
        ("证据", "evidence"),
        ("风险", "risks"),
        ("目标", "goals"),
        ("质量", "quality"),
        ("成果", "result"),
        ("声望奖励", "reputation reward"),
        ("记录", "record"),
        ("确认", "confirm"),
        ("输出", "produce"),
        ("围绕", "around"),
        ("列出", "list"),
        ("补齐", "fill"),
        ("重述", "restate"),
        ("调整", "adjust"),
        ("收紧", "tighten"),
        ("重新打开", "reopen"),
        ("完成", "complete"),
        ("锁定", "lock"),
        ("趁上下文新鲜", "while context is fresh"),
        ("快速", "quickly"),
        ("热度消退前", "before momentum fades"),
        ("埋好", "prepare"),
        ("复述", "restate"),
        ("尽快", "quickly"),
        ("让路线进入", "move the route into"),
        ("和", "and"),
    ];

    let mut translated = value.trim().to_string();
    for (from, to) in replacements {
        translated = translated.replace(from, to);
    }
    let mut normalized_punctuation = String::new();
    for ch in translated.chars() {
        match ch {
            '：' => normalized_punctuation.push_str(": "),
            '，' | '、' => normalized_punctuation.push_str(", "),
            '；' => normalized_punctuation.push_str("; "),
            '。' => normalized_punctuation.push('.'),
            '！' => normalized_punctuation.push('!'),
            '？' => normalized_punctuation.push('?'),
            _ => normalized_punctuation.push(ch),
        }
    }
    translated = normalized_punctuation;

    if contains_cjk_text(&translated) {
        translated = translated
            .chars()
            .filter(|ch| {
                !(('\u{3400}'..='\u{9fff}').contains(ch)
                    || ('\u{f900}'..='\u{faff}').contains(ch)
                    || matches!(ch, '、' | '，' | '。' | '：' | '；' | '！' | '？'))
            })
            .collect::<String>();
    }

    let normalized = translated.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.trim().is_empty() {
        "Route detail pending.".to_string()
    } else {
        normalized
    }
}

struct WorldRouteProjectionContext<'a> {
    world: &'a WorldState,
    indexes: WorldIndexes,
}

impl<'a> WorldRouteProjectionContext<'a> {
    fn new(world: &'a WorldState) -> Self {
        Self {
            world,
            indexes: build_world_indexes(world),
        }
    }

    fn contract_by_id(&self, contract_id: &str) -> Option<&WorldContract> {
        self.indexes.contract(self.world, contract_id)
    }

    fn contract_location_id(&self, contract_id: &str) -> String {
        self.contract_by_id(contract_id)
            .map(|contract| contract.location_id.clone())
            .unwrap_or_default()
    }

    fn contract_task_id(&self, contract_id: &str) -> String {
        self.contract_by_id(contract_id)
            .map(|contract| contract.task_id.clone())
            .unwrap_or_default()
    }

    fn company_location_id(&self, company_id: &str) -> String {
        self.indexes
            .company_location_id(company_id)
            .map(str::to_string)
            .unwrap_or_default()
    }

    fn work_order_location_id(&self, work_order_id: &str) -> String {
        self.indexes
            .work_order_location_by_id
            .get(work_order_id)
            .cloned()
            .unwrap_or_default()
    }

    fn focus_node_id(&self, location_id: &str, panel_id: &str, input_id: &str) -> String {
        let location_id = location_id.trim();
        if location_id.is_empty() {
            return String::new();
        }

        let focus_lane = world_route_focus_lane_spec(panel_id, input_id);

        if let Some(node_id) = focus_lane.preferred_node_id {
            if let Some(node) = self.world.world_map_nodes.get(node_id) {
                if node.location_id == location_id {
                    return node.node_id.clone();
                }
            }
        }

        if !focus_lane.desired_tags.is_empty() {
            if let Some(node) = self.indexes.map_node_for_location_with_tags(
                self.world,
                location_id,
                focus_lane.desired_tags,
            ) {
                return node.node_id.clone();
            }
        }

        self.indexes
            .first_map_node_for_location(self.world, location_id)
            .map(|node| node.node_id.clone())
            .unwrap_or_default()
    }

    fn event_preview_item(&self, event: &WorldEvent) -> Value {
        json!({
            "route_bucket": "event",
            "location_id": event.location_id,
            "task_id": event.cex_task_id,
            "route_status": event.cex_status.as_deref().unwrap_or(&event.result),
            "created_at_epoch": event.created_at_epoch,
            "event_id": event.event_id,
            "title": event.event_kind,
            "summary": event.body,
            "detail": format!("{} · impact +{}", event.location_id, event.impact_score),
        })
    }

    fn contract_preview_item(&self, contract: &WorldContract) -> Value {
        json!({
            "route_bucket": "contract",
            "location_id": contract.location_id,
            "task_id": contract.task_id,
            "route_status": contract.status,
            "created_at_epoch": contract.created_at_epoch,
            "contract_id": contract.contract_id,
            "title": contract.title,
            "summary": contract.body,
            "detail": format!("task {} · value {}", contract.task_id, contract.value_score),
        })
    }

    fn completion_preview_item(&self, completion: &WorldContractCompletion) -> Value {
        let route_status = completion
            .ledger_status
            .clone()
            .unwrap_or_else(|| completion.payout_status.clone());
        json!({
            "route_bucket": "completion",
            "location_id": self.contract_location_id(&completion.contract_id),
            "task_id": self.contract_task_id(&completion.contract_id),
            "route_status": route_status,
            "created_at_epoch": completion.created_at_epoch,
            "completion_id": completion.completion_id,
            "contract_id": completion.contract_id,
            "title": format!("契约战报 {}", completion.completion_id),
            "summary": completion.body,
            "detail": format!("评分 {:.1} · 奖励 {:.2}", completion.score, completion.reward_amount),
        })
    }

    fn purchase_preview_item(&self, purchase: &WorldPurchase) -> Value {
        json!({
            "route_bucket": "purchase",
            "location_id": self.company_location_id(&purchase.company_id),
            "route_status": purchase.status,
            "created_at_epoch": purchase.created_at_epoch,
            "purchase_id": purchase.purchase_id,
            "listing_id": purchase.listing_id,
            "title": format!("接取契约 {}", purchase.purchase_id),
            "summary": format!("{} 奖励 · {}", purchase.price_credits, purchase.status),
            "detail": format!("任务牌 {}", purchase.listing_id),
        })
    }

    fn work_order_preview_item(&self, work_order: &WorldWorkOrder) -> Value {
        json!({
            "route_bucket": "work_order",
            "location_id": self.work_order_location_id(&work_order.work_order_id),
            "route_status": work_order.status,
            "created_at_epoch": work_order.created_at_epoch,
            "work_order_id": work_order.work_order_id,
            "listing_id": work_order.listing_id,
            "purchase_id": work_order.purchase_id,
            "title": format!("冒险委托 {}", work_order.work_order_id),
            "summary": work_order.brief,
            "detail": format!("难度 {} · {}", work_order.value_score, work_order.status),
        })
    }

    fn delivery_preview_item(&self, delivery: &WorldWorkDelivery) -> Value {
        json!({
            "route_bucket": "delivery",
            "location_id": self.work_order_location_id(&delivery.work_order_id),
            "route_status": delivery.status,
            "created_at_epoch": delivery.created_at_epoch,
            "work_order_id": delivery.work_order_id,
            "title": format!("成果提交 {}", delivery.delivery_id),
            "summary": delivery.body,
            "detail": format!("评分 {:.1} · {}", delivery.score, delivery.status),
        })
    }

    fn acceptance_preview_item(&self, acceptance: &WorldWorkAcceptance) -> Value {
        json!({
            "route_bucket": "acceptance",
            "location_id": self.work_order_location_id(&acceptance.work_order_id),
            "route_status": acceptance.status,
            "created_at_epoch": acceptance.created_at_epoch,
            "work_order_id": acceptance.work_order_id,
            "title": format!("评级通过 {}", acceptance.acceptance_id),
            "summary": acceptance.body,
            "detail": format!("声望 +{} · {}", acceptance.reputation_delta, acceptance.status),
        })
    }

    fn rejection_preview_item(&self, rejection: &WorldWorkRejection) -> Value {
        json!({
            "route_bucket": "rejection",
            "location_id": self.work_order_location_id(&rejection.work_order_id),
            "route_status": rejection.status,
            "created_at_epoch": rejection.created_at_epoch,
            "work_order_id": rejection.work_order_id,
            "title": format!("返工要求 {}", rejection.rejection_id),
            "summary": rejection.body,
            "detail": format!("奖励退回 {} · {}", rejection.refund_status, rejection.status),
        })
    }

    fn reopen_preview_item(&self, reopen: &WorldWorkReopen) -> Value {
        json!({
            "route_bucket": "reopen",
            "location_id": self.work_order_location_id(&reopen.work_order_id),
            "route_status": reopen.status,
            "created_at_epoch": reopen.created_at_epoch,
            "work_order_id": reopen.work_order_id,
            "title": format!("委托重开 {}", reopen.reopen_id),
            "summary": reopen.body,
            "detail": format!("再次托管 {} · {}", reopen.reserve_status, reopen.status),
        })
    }

    fn cancellation_preview_item(&self, cancellation: &WorldWorkCancellation) -> Value {
        json!({
            "route_bucket": "cancellation",
            "location_id": self.work_order_location_id(&cancellation.work_order_id),
            "route_status": cancellation.status,
            "created_at_epoch": cancellation.created_at_epoch,
            "work_order_id": cancellation.work_order_id,
            "title": format!("委托放弃 {}", cancellation.cancellation_id),
            "summary": cancellation.body,
            "detail": format!("奖励退回 {} · {}", cancellation.refund_status, cancellation.status),
        })
    }

    fn task_graph_item(&self, task_id: String, mut items: Vec<WorldRoutePreviewItem>) -> Value {
        items.sort_by(|left, right| right.created_at_epoch.cmp(&left.created_at_epoch));
        let latest = items.first().cloned();
        let latest_bucket = latest
            .as_ref()
            .map(|item| item.route_bucket.as_str())
            .unwrap_or("event");
        let latest_status = latest
            .as_ref()
            .map(|item| item.route_status.as_str())
            .unwrap_or("pending");
        let latest_location_id = latest
            .as_ref()
            .map(|item| item.location_id.as_str())
            .unwrap_or("");
        let latest_created_at_epoch = latest
            .as_ref()
            .map(|item| item.created_at_epoch)
            .unwrap_or(0);
        let event_count = items
            .iter()
            .filter(|item| item.route_bucket == "event")
            .count();
        let contract_count = items
            .iter()
            .filter(|item| item.route_bucket == "contract")
            .count();
        let completion_count = items
            .iter()
            .filter(|item| item.route_bucket == "completion")
            .count();
        let latest_event = items.iter().find(|item| item.route_bucket == "event");
        let latest_contract = items.iter().find(|item| item.route_bucket == "contract");
        let latest_completion = items.iter().find(|item| item.route_bucket == "completion");
        let latest_event_id = latest_event
            .map(|item| item.event_id.as_str())
            .unwrap_or("");
        let latest_event_title = latest_event
            .map(|item| item.title.as_str())
            .unwrap_or("world_event");
        let latest_contract_id = latest_contract
            .map(|item| item.contract_id.as_str())
            .unwrap_or("");
        let latest_contract_title = latest_contract
            .map(|item| item.title.as_str())
            .unwrap_or("世界契约");
        let latest_completion_id = latest_completion
            .map(|item| item.completion_id.as_str())
            .unwrap_or("");
        let latest_completion_title = latest_completion
            .map(|item| item.title.as_str())
            .unwrap_or("契约战报");
        let latest_title = latest
            .as_ref()
            .map(|item| item.title.as_str())
            .unwrap_or(latest_bucket);
        let latest_summary = latest
            .as_ref()
            .map(|item| item.summary.as_str())
            .unwrap_or("");
        let latest_detail = latest
            .as_ref()
            .map(|item| item.detail.as_str())
            .unwrap_or("");
        let route_stage_summary = format!(
            "{} 个事件 → {} 份契约 → {} 条战报 · 最新 {}/{}",
            event_count, contract_count, completion_count, latest_bucket, latest_status
        );
        let derivation = WorldRouteTaskDerivationContext {
            task_id: &task_id,
            latest_bucket,
            latest_status,
            latest_location_id,
            latest_title,
            latest_summary,
            latest_detail,
            latest_event_title,
            latest_contract_id,
            latest_contract_title,
            latest_completion_id,
            latest_completion_title,
        };
        let outcome_summary = derivation.outcome_summary();
        let feedback_focus = derivation.feedback_focus();
        let next_opportunity = derivation.next_opportunity();
        let next_opportunity_target = world_route_command_target(&next_opportunity.command);
        let next_opportunity_node_id = self.focus_node_id(
            latest_location_id,
            &next_opportunity_target.panel_id,
            &next_opportunity_target.input_id,
        );
        let suggested_action = derivation.suggested_action();
        let suggested_target = suggested_action.route_target;
        let suggested_node_id = self.focus_node_id(
            latest_location_id,
            &suggested_target.panel_id,
            &suggested_target.input_id,
        );

        json!({
            "task_id": task_id,
            "latest_bucket": latest_bucket,
            "latest_status": latest_status,
            "latest_location_id": latest_location_id,
            "latest_created_at_epoch": latest_created_at_epoch,
            "event_count": event_count,
            "contract_count": contract_count,
            "completion_count": completion_count,
            "latest_event_id": latest_event_id,
            "latest_event_title": latest_event_title,
            "latest_contract_id": latest_contract_id,
            "latest_contract_title": latest_contract_title,
            "latest_completion_id": latest_completion_id,
            "latest_completion_title": latest_completion_title,
            "route_stage_summary": route_stage_summary,
            "outcome_summary": outcome_summary,
            "feedback_focus": feedback_focus,
            "next_opportunity_hint": next_opportunity.hint,
            "next_opportunity_kind": next_opportunity.kind,
            "next_opportunity_playbook": next_opportunity.playbook,
            "next_opportunity_command": next_opportunity.command,
            "next_opportunity_action_label": next_opportunity_target.action_label,
            "next_opportunity_panel_id": next_opportunity_target.panel_id,
            "next_opportunity_input_id": next_opportunity_target.input_id,
            "next_opportunity_input_value": next_opportunity_target.input_value,
            "next_opportunity_textarea_id": next_opportunity_target.textarea_id,
            "next_opportunity_body": next_opportunity_target.body,
            "next_opportunity_node_id": next_opportunity_node_id,
            "suggested_action_label": suggested_target.action_label,
            "suggested_panel_id": suggested_target.panel_id,
            "suggested_input_id": suggested_target.input_id,
            "suggested_input_value": suggested_target.input_value,
            "suggested_textarea_id": suggested_target.textarea_id,
            "suggested_body": suggested_target.body,
            "suggested_matrix_command": suggested_action.matrix_command,
            "suggested_node_id": suggested_node_id,
        })
    }

    fn preview_items(&self) -> Vec<Value> {
        let world = self.world;
        let mut items = Vec::new();

        for event in indexed_recent(&world.world_events, &self.indexes.recent_event_indices, 12) {
            items.push(self.event_preview_item(event));
        }

        for contract in indexed_recent(
            &world.world_contracts,
            &self.indexes.recent_contract_indices,
            12,
        ) {
            items.push(self.contract_preview_item(contract));
        }

        for completion in indexed_recent(
            &world.world_contract_completions,
            &self.indexes.recent_contract_completion_indices,
            12,
        ) {
            items.push(self.completion_preview_item(completion));
        }

        for purchase in indexed_recent(
            &world.world_purchases,
            &self.indexes.recent_purchase_indices,
            8,
        ) {
            items.push(self.purchase_preview_item(purchase));
        }

        for work_order in indexed_recent(
            &world.world_work_orders,
            &self.indexes.recent_work_order_indices,
            8,
        ) {
            items.push(self.work_order_preview_item(work_order));
        }

        for delivery in indexed_recent(
            &world.world_work_deliveries,
            &self.indexes.recent_work_delivery_indices,
            8,
        ) {
            items.push(self.delivery_preview_item(delivery));
        }

        for acceptance in indexed_recent(
            &world.world_work_acceptances,
            &self.indexes.recent_work_acceptance_indices,
            8,
        ) {
            items.push(self.acceptance_preview_item(acceptance));
        }

        for rejection in indexed_recent(
            &world.world_work_rejections,
            &self.indexes.recent_work_rejection_indices,
            8,
        ) {
            items.push(self.rejection_preview_item(rejection));
        }

        for reopen in indexed_recent(
            &world.world_work_reopens,
            &self.indexes.recent_work_reopen_indices,
            8,
        ) {
            items.push(self.reopen_preview_item(reopen));
        }

        for cancellation in indexed_recent(
            &world.world_work_cancellations,
            &self.indexes.recent_work_cancellation_indices,
            8,
        ) {
            items.push(self.cancellation_preview_item(cancellation));
        }

        items
    }

    fn task_graph_items(&self, preview: &Value) -> Vec<Value> {
        let mut tasks: HashMap<String, Vec<WorldRoutePreviewItem>> = HashMap::new();
        for item in world_route_preview_items(preview) {
            let task_id = item.task_group_id();
            if task_id.is_empty() {
                continue;
            }
            tasks.entry(task_id).or_default().push(item);
        }

        tasks
            .into_iter()
            .map(|(task_id, items)| self.task_graph_item(task_id, items))
            .collect()
    }

    fn preview_json(&self) -> Value {
        let mut items = self.preview_items();

        items.sort_by(|left, right| {
            right
                .get("created_at_epoch")
                .and_then(Value::as_i64)
                .cmp(&left.get("created_at_epoch").and_then(Value::as_i64))
        });
        items.truncate(24);

        let task_linked_count = items
            .iter()
            .filter(|item| {
                item.get("task_id")
                    .and_then(Value::as_str)
                    .map(|task_id| !task_id.trim().is_empty())
                    .unwrap_or(false)
            })
            .count();

        json!({
            "projection_layer": "world_route_projection_v1",
            "projection_context": "WorldRouteProjectionContext",
            "index_layer": "WorldIndexes::recent_route_indices_v1",
            "item_count": items.len(),
            "task_linked_count": task_linked_count,
            "items": items,
        })
    }

    fn task_graph_json(&self, preview: &Value) -> Value {
        let mut graph_tasks = self.task_graph_items(preview);

        graph_tasks.sort_by(|left, right| {
            right
                .get("latest_created_at_epoch")
                .and_then(Value::as_i64)
                .cmp(&left.get("latest_created_at_epoch").and_then(Value::as_i64))
        });

        json!({
            "projection_layer": "world_route_task_graph_projection_v1",
            "projection_context": "WorldRouteProjectionContext",
            "task_count": graph_tasks.len(),
            "tasks": graph_tasks,
        })
    }

    fn artifacts(&self) -> WorldRouteArtifacts {
        let preview = self.preview_json();
        let task_graph = self.task_graph_json(&preview);
        let task_views = world_route_task_graph_views(&task_graph, 24);
        let story = WorldRouteStoryView::from_route_data(&preview, &task_graph, &task_views);
        WorldRouteArtifacts {
            preview,
            task_graph,
            task_views,
            story,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct WorldRouteCommandTarget {
    pub(super) action_label: String,
    pub(super) panel_id: String,
    pub(super) input_id: String,
    pub(super) input_value: String,
    pub(super) textarea_id: String,
    pub(super) body: String,
}

impl WorldRouteCommandTarget {
    fn lane(
        action_label: &str,
        panel_id: &str,
        input_id: &str,
        input_value: String,
        textarea_id: &str,
        body: String,
    ) -> Self {
        Self {
            action_label: action_label.to_string(),
            panel_id: panel_id.to_string(),
            input_id: input_id.to_string(),
            input_value,
            textarea_id: textarea_id.to_string(),
            body,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ClientFeedActionTarget {
    action_label: Value,
    panel_id: Value,
    input_id: Value,
    input_value: Value,
    textarea_id: Value,
    location_id: Value,
    target_node_id: Value,
    task_id: Value,
    contract_id: Value,
    listing_id: Value,
    work_order_id: Value,
    event_id: Value,
    event_kind: Value,
    event_body: Value,
    event_result: Value,
    event_task_id: Value,
    body_base: Value,
}

impl ClientFeedActionTarget {
    pub(super) fn from_values(
        action_label: Value,
        panel_id: Value,
        input_id: Value,
        input_value: Value,
        textarea_id: Value,
        body_base: Value,
    ) -> Self {
        Self {
            action_label,
            panel_id,
            input_id,
            input_value,
            textarea_id,
            location_id: json!(""),
            target_node_id: json!(""),
            task_id: json!(""),
            contract_id: json!(""),
            listing_id: json!(""),
            work_order_id: json!(""),
            event_id: json!(""),
            event_kind: json!(""),
            event_body: json!(""),
            event_result: json!(""),
            event_task_id: json!(""),
            body_base,
        }
    }

    pub(super) fn from_route_target(target: WorldRouteCommandTarget) -> Self {
        Self::from_values(
            json!(target.action_label),
            json!(target.panel_id),
            json!(target.input_id),
            json!(target.input_value),
            json!(target.textarea_id),
            json!(target.body),
        )
    }

    pub(super) fn with_location_id(mut self, value: Value) -> Self {
        self.location_id = value;
        self
    }

    pub(super) fn with_target_node_id(mut self, value: Value) -> Self {
        self.target_node_id = value;
        self
    }

    pub(super) fn with_task_id(mut self, value: Value) -> Self {
        self.task_id = value;
        self
    }

    pub(super) fn with_contract_id(mut self, value: Value) -> Self {
        self.contract_id = value;
        self
    }

    pub(super) fn with_listing_id(mut self, value: Value) -> Self {
        self.listing_id = value;
        self
    }

    pub(super) fn with_work_order_id(mut self, value: Value) -> Self {
        self.work_order_id = value;
        self
    }

    pub(super) fn with_event_details(
        mut self,
        event_id: Value,
        event_kind: Value,
        event_body: Value,
        event_result: Value,
        event_task_id: Value,
    ) -> Self {
        self.event_id = event_id;
        self.event_kind = event_kind;
        self.event_body = event_body;
        self.event_result = event_result;
        self.event_task_id = event_task_id;
        self
    }

    pub(super) fn apply(self, object: &mut serde_json::Map<String, Value>) {
        object.insert("action_label".to_string(), self.action_label);
        object.insert("action_panel_id".to_string(), self.panel_id);
        object.insert("action_input_id".to_string(), self.input_id);
        object.insert("action_input_value".to_string(), self.input_value);
        object.insert("action_textarea_id".to_string(), self.textarea_id);
        object.insert("action_location_id".to_string(), self.location_id);
        object.insert("action_target_node_id".to_string(), self.target_node_id);
        object.insert("action_task_id".to_string(), self.task_id);
        object.insert("action_contract_id".to_string(), self.contract_id);
        object.insert("action_listing_id".to_string(), self.listing_id);
        object.insert("action_work_order_id".to_string(), self.work_order_id);
        object.insert("action_event_id".to_string(), self.event_id);
        object.insert("action_event_kind".to_string(), self.event_kind);
        object.insert("action_event_body".to_string(), self.event_body);
        object.insert("action_event_result".to_string(), self.event_result);
        object.insert("action_event_task_id".to_string(), self.event_task_id);
        object.insert("action_body_base".to_string(), self.body_base);
    }
}

#[derive(Debug, Clone, Copy)]
struct WorldRouteFocusLaneSpec {
    preferred_node_id: Option<&'static str>,
    desired_tags: &'static [&'static str],
}

pub(super) const WORLD_ROUTE_WORK_DELIVER_INPUT_ID: &str = "world-work-deliver-id";
pub(super) const WORLD_ROUTE_WORK_DELIVER_TEXTAREA_ID: &str = "world-work-deliver-body";
pub(super) const WORLD_ROUTE_WORK_ACCEPT_INPUT_ID: &str = "world-work-accept-id";
pub(super) const WORLD_ROUTE_WORK_ACCEPT_TEXTAREA_ID: &str = "world-work-accept-body";
pub(super) const WORLD_ROUTE_WORK_REJECT_INPUT_ID: &str = "world-work-reject-id";
pub(super) const WORLD_ROUTE_WORK_REJECT_TEXTAREA_ID: &str = "world-work-reject-body";
pub(super) const WORLD_ROUTE_WORK_REOPEN_INPUT_ID: &str = "world-work-reopen-id";
pub(super) const WORLD_ROUTE_WORK_REOPEN_TEXTAREA_ID: &str = "world-work-reopen-body";
pub(super) const WORLD_ROUTE_WORK_CANCEL_INPUT_ID: &str = "world-work-cancel-id";
pub(super) const WORLD_ROUTE_WORK_CANCEL_TEXTAREA_ID: &str = "world-work-cancel-body";
pub(super) const WORLD_ROUTE_WORK_LANE_INPUT_IDS: &[&str] = &[
    WORLD_ROUTE_WORK_DELIVER_INPUT_ID,
    WORLD_ROUTE_WORK_ACCEPT_INPUT_ID,
    WORLD_ROUTE_WORK_REJECT_INPUT_ID,
    WORLD_ROUTE_WORK_REOPEN_INPUT_ID,
    WORLD_ROUTE_WORK_CANCEL_INPUT_ID,
];
pub(super) const WORLD_ROUTE_MAP_MOVE_PANEL_ID: &str = "world-map-move-panel";
pub(super) const WORLD_ROUTE_MOVE_TARGET_ID: &str = "world-map-move-target";
pub(super) const WORLD_ROUTE_ACTION_PANEL_ID: &str = "world-action-console";
pub(super) const WORLD_ROUTE_ACTION_LOCATION_ID: &str = "world-action-location";
pub(super) const WORLD_ROUTE_ACTION_TEXTAREA_ID: &str = "world-action-body";
pub(super) const WORLD_ROUTE_ASSETS_PANEL_ID: &str = "world-assets-panel";
pub(super) const WORLD_ROUTE_ASSET_INPUT_ID: &str = "world-asset-id";
pub(super) const WORLD_ROUTE_ASSET_TEXTAREA_ID: &str = "world-asset-body";
pub(super) const WORLD_ROUTE_COMPANIES_PANEL_ID: &str = "world-companies-panel";
pub(super) const WORLD_ROUTE_COMPANY_INPUT_ID: &str = "world-company-asset-id";
pub(super) const WORLD_ROUTE_COMPANY_TEXTAREA_ID: &str = "world-company-body";
pub(super) const WORLD_ROUTE_LISTINGS_PANEL_ID: &str = "world-listings-panel";
pub(super) const WORLD_ROUTE_LISTING_INPUT_ID: &str = "world-listing-company-id";
pub(super) const WORLD_ROUTE_LISTING_TEXTAREA_ID: &str = "world-listing-body";
pub(super) const WORLD_ROUTE_COMMERCE_PANEL_ID: &str = "world-commerce-panel";
pub(super) const WORLD_ROUTE_CONTRACTS_PANEL_ID: &str = "world-contracts-panel";
pub(super) const WORLD_ROUTE_PURCHASE_INPUT_ID: &str = "world-buy-listing-id";
pub(super) const WORLD_ROUTE_PURCHASE_TEXTAREA_ID: &str = "world-buy-body";
pub(super) const WORLD_ROUTE_CONTRACT_INPUT_ID: &str = "world-contract-completion-id";
pub(super) const WORLD_ROUTE_CONTRACT_TEXTAREA_ID: &str = "world-contract-completion-body";
pub(super) const WORLD_ROUTE_EVENT_TIMELINE_ID: &str = "world-event-timeline";
pub(super) const WORLD_ROUTE_LEAGUE_LINK_ID: &str = "world-league-link";

pub(super) fn world_route_ui_contract_json() -> Value {
    let mut panel_defaults = serde_json::Map::new();
    panel_defaults.insert(
        WORLD_ROUTE_MAP_MOVE_PANEL_ID.to_string(),
        json!({
            "input_id": WORLD_ROUTE_MOVE_TARGET_ID,
        }),
    );
    panel_defaults.insert(
        WORLD_ROUTE_ACTION_PANEL_ID.to_string(),
        json!({
            "textarea_id": WORLD_ROUTE_ACTION_TEXTAREA_ID,
        }),
    );
    panel_defaults.insert(
        WORLD_ROUTE_ASSETS_PANEL_ID.to_string(),
        json!({
            "input_id": WORLD_ROUTE_ASSET_INPUT_ID,
            "textarea_id": WORLD_ROUTE_ASSET_TEXTAREA_ID,
        }),
    );
    panel_defaults.insert(
        WORLD_ROUTE_COMPANIES_PANEL_ID.to_string(),
        json!({
            "input_id": WORLD_ROUTE_COMPANY_INPUT_ID,
            "textarea_id": WORLD_ROUTE_COMPANY_TEXTAREA_ID,
        }),
    );
    panel_defaults.insert(
        WORLD_ROUTE_LISTINGS_PANEL_ID.to_string(),
        json!({
            "input_id": WORLD_ROUTE_LISTING_INPUT_ID,
            "textarea_id": WORLD_ROUTE_LISTING_TEXTAREA_ID,
        }),
    );
    panel_defaults.insert(
        WORLD_ROUTE_COMMERCE_PANEL_ID.to_string(),
        json!({
            "input_id": WORLD_ROUTE_PURCHASE_INPUT_ID,
            "textarea_id": WORLD_ROUTE_PURCHASE_TEXTAREA_ID,
        }),
    );
    panel_defaults.insert(
        WORLD_ROUTE_CONTRACTS_PANEL_ID.to_string(),
        json!({
            "input_id": WORLD_ROUTE_CONTRACT_INPUT_ID,
            "textarea_id": WORLD_ROUTE_CONTRACT_TEXTAREA_ID,
        }),
    );
    Value::Object(serde_json::Map::from_iter([
        ("contract_version".to_string(), json!(1)),
        (
            "panels".to_string(),
            json!({
                "map_move": WORLD_ROUTE_MAP_MOVE_PANEL_ID,
                "action": WORLD_ROUTE_ACTION_PANEL_ID,
                "assets": WORLD_ROUTE_ASSETS_PANEL_ID,
                "companies": WORLD_ROUTE_COMPANIES_PANEL_ID,
                "listings": WORLD_ROUTE_LISTINGS_PANEL_ID,
                "commerce": WORLD_ROUTE_COMMERCE_PANEL_ID,
                "contracts": WORLD_ROUTE_CONTRACTS_PANEL_ID,
                "event_timeline": WORLD_ROUTE_EVENT_TIMELINE_ID,
                "league_link": WORLD_ROUTE_LEAGUE_LINK_ID,
            }),
        ),
        (
            "fields".to_string(),
            json!({
                "move_target": WORLD_ROUTE_MOVE_TARGET_ID,
                "action_location": WORLD_ROUTE_ACTION_LOCATION_ID,
                "action_textarea": WORLD_ROUTE_ACTION_TEXTAREA_ID,
                "asset_input": WORLD_ROUTE_ASSET_INPUT_ID,
                "asset_textarea": WORLD_ROUTE_ASSET_TEXTAREA_ID,
                "company_input": WORLD_ROUTE_COMPANY_INPUT_ID,
                "company_textarea": WORLD_ROUTE_COMPANY_TEXTAREA_ID,
                "listing_input": WORLD_ROUTE_LISTING_INPUT_ID,
                "listing_textarea": WORLD_ROUTE_LISTING_TEXTAREA_ID,
                "purchase_input": WORLD_ROUTE_PURCHASE_INPUT_ID,
                "purchase_textarea": WORLD_ROUTE_PURCHASE_TEXTAREA_ID,
                "contract_input": WORLD_ROUTE_CONTRACT_INPUT_ID,
                "contract_textarea": WORLD_ROUTE_CONTRACT_TEXTAREA_ID,
            }),
        ),
        ("panel_defaults".to_string(), Value::Object(panel_defaults)),
        (
            "work_lane_order".to_string(),
            json!([
                "delivery",
                "acceptance",
                "rejection",
                "reopen",
                "cancellation"
            ]),
        ),
        (
            "work_lanes".to_string(),
            json!({
                "delivery": {
                    "input_id": WORLD_ROUTE_WORK_DELIVER_INPUT_ID,
                    "textarea_id": WORLD_ROUTE_WORK_DELIVER_TEXTAREA_ID,
                },
                "acceptance": {
                    "input_id": WORLD_ROUTE_WORK_ACCEPT_INPUT_ID,
                    "textarea_id": WORLD_ROUTE_WORK_ACCEPT_TEXTAREA_ID,
                },
                "rejection": {
                    "input_id": WORLD_ROUTE_WORK_REJECT_INPUT_ID,
                    "textarea_id": WORLD_ROUTE_WORK_REJECT_TEXTAREA_ID,
                },
                "reopen": {
                    "input_id": WORLD_ROUTE_WORK_REOPEN_INPUT_ID,
                    "textarea_id": WORLD_ROUTE_WORK_REOPEN_TEXTAREA_ID,
                },
                "cancellation": {
                    "input_id": WORLD_ROUTE_WORK_CANCEL_INPUT_ID,
                    "textarea_id": WORLD_ROUTE_WORK_CANCEL_TEXTAREA_ID,
                },
            }),
        ),
        (
            "handoff".to_string(),
            json!({
                "storage_key": "trillionnium-world-handoff",
                "saved_at_epoch": "saved_at_epoch",
                "action_label": "action_label",
                "action_id": "action_id",
                "command": "command",
                "location_id": "location_id",
                "node_id": "node_id",
                "panel_id": "web_panel_id",
                "action_body": "web_action_body",
                "target_input_id": "web_target_input_id",
                "target_value": "web_target_value",
                "target_textarea_id": "web_target_textarea_id",
                "move_target": "web_move_target",
                "listing_id": "web_listing_id",
                "work_order_id": "web_work_order_id",
                "contract_id": "web_contract_id",
                "route_task_id": "web_route_task_id",
                "event_id": "web_event_id",
                "event_kind": "web_event_kind",
                "event_body": "web_event_body",
                "event_result": "web_event_result",
            }),
        ),
    ]))
}

fn is_world_work_lane_input(input_id: &str) -> bool {
    WORLD_ROUTE_WORK_LANE_INPUT_IDS.contains(&input_id)
}

fn world_route_focus_lane_spec(panel_id: &str, input_id: &str) -> WorldRouteFocusLaneSpec {
    match (panel_id, input_id) {
        (WORLD_ROUTE_ASSETS_PANEL_ID, _) => WorldRouteFocusLaneSpec {
            preferred_node_id: Some("asset-yard"),
            desired_tags: &["asset", "upgrade", "inventory"],
        },
        (WORLD_ROUTE_COMPANIES_PANEL_ID, _) => WorldRouteFocusLaneSpec {
            preferred_node_id: Some("starter-studio"),
            desired_tags: &["company", "craft", "decorate"],
        },
        (WORLD_ROUTE_LISTINGS_PANEL_ID, _) => WorldRouteFocusLaneSpec {
            preferred_node_id: Some("client-board"),
            desired_tags: &["listing", "brief", "market", "sell"],
        },
        (WORLD_ROUTE_COMMERCE_PANEL_ID, WORLD_ROUTE_PURCHASE_INPUT_ID) => WorldRouteFocusLaneSpec {
            preferred_node_id: Some("zbj-market-gate"),
            desired_tags: &["market", "buy", "sell", "listing"],
        },
        (WORLD_ROUTE_COMMERCE_PANEL_ID, input_id) if is_world_work_lane_input(input_id) => {
            WorldRouteFocusLaneSpec {
                preferred_node_id: Some("delivery-dock"),
                desired_tags: &["deliver", "accept", "reject", "cancel", "refund", "review"],
            }
        }
        (WORLD_ROUTE_CONTRACTS_PANEL_ID, _) => WorldRouteFocusLaneSpec {
            preferred_node_id: Some("ledger-office"),
            desired_tags: &["contract", "ledger", "refund"],
        },
        _ => WorldRouteFocusLaneSpec {
            preferred_node_id: None,
            desired_tags: &[],
        },
    }
}

pub(super) fn world_route_action_console_target(
    action_label: &str,
    body: String,
) -> WorldRouteCommandTarget {
    WorldRouteCommandTarget::lane(
        action_label,
        WORLD_ROUTE_ACTION_PANEL_ID,
        "",
        String::new(),
        WORLD_ROUTE_ACTION_TEXTAREA_ID,
        body,
    )
}

pub(super) fn world_route_asset_lane_target(
    action_label: &str,
    asset_id: String,
    body: String,
) -> WorldRouteCommandTarget {
    WorldRouteCommandTarget::lane(
        action_label,
        WORLD_ROUTE_ASSETS_PANEL_ID,
        WORLD_ROUTE_ASSET_INPUT_ID,
        asset_id,
        WORLD_ROUTE_ASSET_TEXTAREA_ID,
        body,
    )
}

pub(super) fn world_route_company_lane_target(
    action_label: &str,
    asset_id: String,
    body: String,
) -> WorldRouteCommandTarget {
    WorldRouteCommandTarget::lane(
        action_label,
        WORLD_ROUTE_COMPANIES_PANEL_ID,
        WORLD_ROUTE_COMPANY_INPUT_ID,
        asset_id,
        WORLD_ROUTE_COMPANY_TEXTAREA_ID,
        body,
    )
}

pub(super) fn world_route_listing_lane_target(
    action_label: &str,
    company_id: String,
    body: String,
) -> WorldRouteCommandTarget {
    WorldRouteCommandTarget::lane(
        action_label,
        WORLD_ROUTE_LISTINGS_PANEL_ID,
        WORLD_ROUTE_LISTING_INPUT_ID,
        company_id,
        WORLD_ROUTE_LISTING_TEXTAREA_ID,
        body,
    )
}

pub(super) fn world_route_purchase_lane_target(
    action_label: &str,
    listing_id: String,
    body: String,
) -> WorldRouteCommandTarget {
    WorldRouteCommandTarget::lane(
        action_label,
        WORLD_ROUTE_COMMERCE_PANEL_ID,
        WORLD_ROUTE_PURCHASE_INPUT_ID,
        listing_id,
        WORLD_ROUTE_PURCHASE_TEXTAREA_ID,
        body,
    )
}

fn world_route_work_lane_ids(lane_kind: &str) -> (&'static str, &'static str) {
    match lane_kind {
        "acceptance" => (
            WORLD_ROUTE_WORK_ACCEPT_INPUT_ID,
            WORLD_ROUTE_WORK_ACCEPT_TEXTAREA_ID,
        ),
        "rejection" => (
            WORLD_ROUTE_WORK_REJECT_INPUT_ID,
            WORLD_ROUTE_WORK_REJECT_TEXTAREA_ID,
        ),
        "reopen" => (
            WORLD_ROUTE_WORK_REOPEN_INPUT_ID,
            WORLD_ROUTE_WORK_REOPEN_TEXTAREA_ID,
        ),
        "cancellation" => (
            WORLD_ROUTE_WORK_CANCEL_INPUT_ID,
            WORLD_ROUTE_WORK_CANCEL_TEXTAREA_ID,
        ),
        _ => (
            WORLD_ROUTE_WORK_DELIVER_INPUT_ID,
            WORLD_ROUTE_WORK_DELIVER_TEXTAREA_ID,
        ),
    }
}

pub(super) fn world_route_work_lane_target(
    action_label: &str,
    input_id: &str,
    textarea_id: &str,
    body: String,
) -> WorldRouteCommandTarget {
    WorldRouteCommandTarget::lane(
        action_label,
        WORLD_ROUTE_COMMERCE_PANEL_ID,
        input_id,
        "latest".to_string(),
        textarea_id,
        body,
    )
}

pub(super) fn world_route_work_lane_target_by_kind(
    action_label: &str,
    lane_kind: &str,
    body: String,
) -> WorldRouteCommandTarget {
    let (input_id, textarea_id) = world_route_work_lane_ids(lane_kind);
    world_route_work_lane_target(action_label, input_id, textarea_id, body)
}

pub(super) fn world_route_contract_lane_target(
    action_label: &str,
    contract_id: String,
    body: String,
) -> WorldRouteCommandTarget {
    WorldRouteCommandTarget::lane(
        action_label,
        WORLD_ROUTE_CONTRACTS_PANEL_ID,
        WORLD_ROUTE_CONTRACT_INPUT_ID,
        contract_id,
        WORLD_ROUTE_CONTRACT_TEXTAREA_ID,
        body,
    )
}

type WorldRouteCommandTargetBuilder = fn(String) -> WorldRouteCommandTarget;

fn world_route_upgrade_latest_command_target(body: String) -> WorldRouteCommandTarget {
    world_route_asset_lane_target("打开道具升级路线", "latest".to_string(), body)
}

fn world_route_company_latest_command_target(body: String) -> WorldRouteCommandTarget {
    world_route_company_lane_target("打开工坊路线", "latest".to_string(), body)
}

fn world_route_sell_latest_command_target(body: String) -> WorldRouteCommandTarget {
    world_route_listing_lane_target("打开任务牌路线", "latest".to_string(), body)
}

fn world_route_buy_latest_command_target(body: String) -> WorldRouteCommandTarget {
    world_route_purchase_lane_target("打开接取路线", "latest".to_string(), body)
}

fn world_route_work_deliver_latest_command_target(body: String) -> WorldRouteCommandTarget {
    world_route_work_lane_target_by_kind("打开成果提交路线", "delivery", body)
}

fn world_route_work_accept_latest_command_target(body: String) -> WorldRouteCommandTarget {
    world_route_work_lane_target_by_kind("打开评级路线", "acceptance", body)
}

fn world_route_work_reject_latest_command_target(body: String) -> WorldRouteCommandTarget {
    world_route_work_lane_target_by_kind("打开返工路线", "rejection", body)
}

fn world_route_work_reopen_latest_command_target(body: String) -> WorldRouteCommandTarget {
    world_route_work_lane_target_by_kind("打开重开路线", "reopen", body)
}

fn world_route_work_cancel_latest_command_target(body: String) -> WorldRouteCommandTarget {
    world_route_work_lane_target_by_kind("打开放弃路线", "cancellation", body)
}

pub(super) const WORLD_ROUTE_PREFIX_COMMAND_BUILDERS: &[(&str, WorldRouteCommandTargetBuilder)] = &[
    ("/upgrade latest", world_route_upgrade_latest_command_target),
    ("/company latest", world_route_company_latest_command_target),
    ("/sell latest", world_route_sell_latest_command_target),
    ("/buy latest", world_route_buy_latest_command_target),
    (
        "/work deliver latest",
        world_route_work_deliver_latest_command_target,
    ),
    (
        "/work accept latest",
        world_route_work_accept_latest_command_target,
    ),
    (
        "/work reject latest",
        world_route_work_reject_latest_command_target,
    ),
    (
        "/work reopen latest",
        world_route_work_reopen_latest_command_target,
    ),
    (
        "/work cancel latest",
        world_route_work_cancel_latest_command_target,
    ),
];

fn finalize_world_route_command_target(
    mut target: WorldRouteCommandTarget,
    fallback_body: &str,
) -> WorldRouteCommandTarget {
    if target.body.trim().is_empty() {
        target.body = fallback_body.trim().to_string();
    }
    target
}

pub(super) fn world_route_command_target(command: &str) -> WorldRouteCommandTarget {
    let trimmed = command.trim();
    let strip_body = |prefix: &str| {
        trimmed
            .strip_prefix(prefix)
            .map(|body| body.trim().to_string())
    };

    for (prefix, builder) in WORLD_ROUTE_PREFIX_COMMAND_BUILDERS {
        if let Some(body) = strip_body(prefix) {
            return finalize_world_route_command_target(builder(body), trimmed);
        }
    }

    if let Some(body) = strip_body("/world action") {
        return finalize_world_route_command_target(
            world_route_action_console_target("打开世界行动路线", body),
            trimmed,
        );
    }

    if let Some(body) = strip_body("/contract") {
        return finalize_world_route_command_target(
            world_route_action_console_target("打开契约捕捉路线", body),
            trimmed,
        );
    }

    if let Some(rest) = trimmed.strip_prefix("/complete ") {
        let rest = rest.trim();
        let (contract_id, body) = rest
            .split_once(' ')
            .map(|(contract_id, body)| (contract_id.trim(), body.trim().to_string()))
            .unwrap_or((rest, String::new()));
        return finalize_world_route_command_target(
            world_route_contract_lane_target("打开契约完成路线", contract_id.to_string(), body),
            trimmed,
        );
    }

    finalize_world_route_command_target(
        world_route_action_console_target("打开世界行动路线", trimmed.to_string()),
        trimmed,
    )
}

#[derive(Debug, Clone)]
struct WorldRouteOpportunity {
    kind: String,
    hint: String,
    playbook: String,
    command: String,
}

#[derive(Debug, Clone)]
struct WorldRouteSuggestedAction {
    route_target: WorldRouteCommandTarget,
    matrix_command: String,
}

#[derive(Debug, Clone, Copy)]
struct WorldRouteTaskDerivationContext<'a> {
    task_id: &'a str,
    latest_bucket: &'a str,
    latest_status: &'a str,
    latest_location_id: &'a str,
    latest_title: &'a str,
    latest_summary: &'a str,
    latest_detail: &'a str,
    latest_event_title: &'a str,
    latest_contract_id: &'a str,
    latest_contract_title: &'a str,
    latest_completion_id: &'a str,
    latest_completion_title: &'a str,
}

impl<'a> WorldRouteTaskDerivationContext<'a> {
    fn outcome_summary(&self) -> String {
        match self.latest_bucket {
            "completion" if !self.latest_completion_id.is_empty() => format!(
                "{} · {}{}{}",
                self.latest_completion_title,
                self.latest_status,
                if self.latest_detail.is_empty() {
                    ""
                } else {
                    " · "
                },
                self.latest_detail
            ),
            "contract" if !self.latest_contract_id.is_empty() => format!(
                "{} · {}{}{}",
                self.latest_contract_title,
                self.latest_status,
                if self.latest_detail.is_empty() {
                    ""
                } else {
                    " · "
                },
                self.latest_detail
            ),
            _ => format!(
                "{} · {}{}{}",
                self.latest_title,
                self.latest_status,
                if self.latest_detail.is_empty() {
                    ""
                } else {
                    " · "
                },
                self.latest_detail
            ),
        }
    }

    fn feedback_focus(&self) -> String {
        match self.latest_bucket {
            "completion" => format!(
                "记录 {} 的委托方反馈，并归档最终成果证据、评分和复盘。",
                self.latest_completion_title
            ),
            "acceptance" => format!(
                "记录委托方在 {} 中通过了什么，以及哪些证据提升了信任。",
                self.latest_title
            ),
            "rejection" => format!(
                "记录 {} 的证据缺口、委托方异议和返工范围。",
                self.latest_title
            ),
            "reopen" => format!(
                "记录 {} 为什么重开、评级标准如何变化，以及下一步要修什么。",
                self.latest_title
            ),
            "cancellation" => format!(
                "记录 {} 的放弃原因，以及下次如何提前校准需求来减少流失。",
                self.latest_title
            ),
            "contract" => format!(
                "记录 {} 的成果计划、评级标准和未清风险。",
                self.latest_contract_title
            ),
            "delivery" => format!(
                "记录 {} 的已提交成果、缺失证据和委托方评级线索。",
                self.latest_title
            ),
            "purchase" | "work_order" => format!(
                "记录 {} 的委托简报、承诺范围和执行风险。",
                self.latest_title
            ),
            _ => format!(
                "记录 {}{}{} 的世界状态证据、阻碍和下一步行动。",
                self.latest_title,
                if self.latest_summary.is_empty() {
                    ""
                } else {
                    " · "
                },
                self.latest_summary
            ),
        }
    }

    fn next_opportunity(&self) -> WorldRouteOpportunity {
        match self.latest_bucket {
            "completion" => WorldRouteOpportunity {
                kind: "repeat_order_upsell_referral".to_string(),
                hint: format!(
                    "把 {} 作为下一条回访委托、升级悬赏或转介绍支线的跳板{}{}。",
                    self.latest_completion_title,
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                playbook: format!(
                    "归档 {} 的成果证据，索取评价/转介绍，再包装更高价值的回访委托或升级悬赏{}{}。",
                    self.latest_completion_title,
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                command: format!(
                    "/sell latest 回访/升级悬赏：基于 {} 提供下一阶段成果、证据、赏金、评级标准、时间线和推荐理由。",
                    self.latest_completion_title
                ),
            },
            "acceptance" => WorldRouteOpportunity {
                kind: "acceptance_upsell".to_string(),
                hint: format!(
                    "把 {} 转化为下一次协作、评价或高阶悬赏{}{}。",
                    self.latest_title,
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                playbook: format!(
                    "记录 {} 为什么获得通过，把证据转成评价，再提出更高价值的后续支线{}{}。",
                    self.latest_title,
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                command: format!(
                    "/sell latest 评级后升级悬赏：围绕 {} 输出高阶范围、证据、赏金阶梯、时间线和下一步。",
                    self.latest_title
                ),
            },
            "rejection" => WorldRouteOpportunity {
                kind: "revision_recovery".to_string(),
                hint: format!(
                    "调整委托方案，收紧评级标准，并重新打开下一条支线{}{}。",
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                playbook: format!(
                    "列出 {} 的异议，补齐缺失证据，重述评级标准，再重新提交成果并打开下一条支线。",
                    self.latest_title
                ),
                command: "/work deliver latest 修订成果：补齐证据、修复缺口、重新对齐评级标准、风险复盘和下一步。".to_string(),
            },
            "reopen" => WorldRouteOpportunity {
                kind: "reopen_recovery".to_string(),
                hint: format!(
                    "调整委托方案，收紧评级标准，并重新打开下一条支线{}{}。",
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                playbook: format!(
                    "用 {} 推动可控的再次提交循环：锁定新评级线，补齐证据缺口，再在通过后打开扩展支线。",
                    self.latest_title
                ),
                command: "/work deliver latest 重开后修订成果：补齐证据、修复重开要求、更新评级清单和下一步。".to_string(),
            },
            "cancellation" => WorldRouteOpportunity {
                kind: "smaller_scope_requalification".to_string(),
                hint: format!(
                    "用更小范围或更清晰的委托机会恢复这条路线{}{}。",
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                playbook: format!(
                    "把 {} 转成重新校准：缩小范围、降低风险、明确证据，再用更小的起步悬赏重启。",
                    self.latest_title
                ),
                command: "/sell latest 小范围试炼委托：更小范围、明确成果/证据、低风险评级标准、赏金和下一步。".to_string(),
            },
            "contract" => WorldRouteOpportunity {
                kind: "delivery_then_upsell".to_string(),
                hint: format!(
                    "先完成 {}，再打开成果后的跟进支线和下一条悬赏{}{}。",
                    self.latest_contract_title,
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                playbook: format!(
                    "完成 {}，锁定成果证据和评级标准，再趁上下文新鲜起草下一条支线。",
                    self.latest_contract_title
                ),
                command: format!(
                    "/complete {} 成果方案：包含成果、证据、风险复盘、评级标准、下一步和自检记录。",
                    self.latest_contract_id
                ),
            },
            "delivery" => WorldRouteOpportunity {
                kind: "acceptance_closeout".to_string(),
                hint: format!(
                    "用 {} 完成评级闭环、沉淀证据，并铺好下一次协作{}{}。",
                    self.latest_title,
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                playbook: format!(
                    "推动 {} 完成委托方评级：突出证据，快速补齐缺口，在热度消退前埋好下一条支线。",
                    self.latest_title
                ),
                command: "/work accept latest 评级通过：确认成果、证据、质量、下一次协作和声望奖励。".to_string(),
            },
            "purchase" | "work_order" => WorldRouteOpportunity {
                kind: "fulfillment_launch".to_string(),
                hint: format!(
                    "把 {} 推进成下一份契约、任务牌或世界行动机会{}{}。",
                    self.latest_title,
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                playbook: format!(
                    "复述 {} 的委托目标，锁定里程碑和证据，尽快提交第一轮成果，让路线进入评级和下一条支线。",
                    self.latest_title
                ),
                command: "/work deliver latest 首轮成果：成果、证据、评级清单、风险复盘、下一步。".to_string(),
            },
            _ => WorldRouteOpportunity {
                kind: "contract_capture".to_string(),
                hint: format!(
                    "把 {} 转化为下一份契约、任务牌或世界行动机会{}{}。",
                    self.latest_title,
                    if self.latest_location_id.is_empty() { "" } else { " @ " },
                    self.latest_location_id
                ),
                playbook: format!(
                    "判断 {} 的可行性，记录证据和风险，再决定转成契约、任务牌或直接世界行动。",
                    self.latest_title
                ),
                command: format!(
                    "/contract 围绕 {} 整理目标、证据、风险、评级标准和下一步。",
                    self.latest_title
                ),
            },
        }
    }

    fn suggested_action(&self) -> WorldRouteSuggestedAction {
        if self.latest_bucket == "completion" && !self.latest_completion_id.is_empty() {
            WorldRouteSuggestedAction {
                route_target: world_route_action_console_target(
                    "起草战报后续",
                    format!(
                        "任务 {}：跟进战报 {}，记录成果证据、委托方反馈和下一条支线。",
                        self.task_id, self.latest_completion_id
                    ),
                ),
                matrix_command: format!(
                    "/world action 跟进已完成任务 {}：围绕 {} 记录成果证据、委托方反馈、复盘和下一条支线。",
                    self.task_id, self.latest_completion_title
                ),
            }
        } else if !self.latest_contract_id.is_empty() {
            WorldRouteSuggestedAction {
                route_target: world_route_contract_lane_target(
                    "打开关联契约",
                    self.latest_contract_id.to_string(),
                    format!(
                        "任务 {}：完成关联契约 {}，带上证据、评级标准和下一步。",
                        self.task_id, self.latest_contract_id
                    ),
                ),
                matrix_command: format!(
                    "/complete {} 成果方案：包含成果、证据、风险复盘、评级标准、下一步和自检记录。",
                    self.latest_contract_id
                ),
            }
        } else {
            WorldRouteSuggestedAction {
                route_target: world_route_action_console_target(
                    "起草任务后续",
                    format!(
                        "任务 {}：跟进关联事件 {}，记录世界状态证据、阻碍和下一步行动。",
                        self.task_id, self.latest_event_title
                    ),
                ),
                matrix_command: format!(
                    "/world action 跟进任务 {}：记录 {} 的证据、阻塞和下一步。",
                    self.task_id, self.latest_event_title
                ),
            }
        }
    }
}

#[derive(Debug, Clone)]
struct WorldRouteStoryOpportunityTarget {
    action_label: String,
    panel_id: String,
    input_id: String,
    input_value: String,
    textarea_id: String,
    body: String,
    node_id: String,
}

#[derive(Debug, Clone)]
pub(super) struct WorldRouteStoryView {
    preview_item_count: u64,
    task_linked_count: u64,
    task_graph_count: u64,
    next_task_id: String,
    next_action_label: String,
    next_panel_id: String,
    next_command_hint: String,
    next_location_id: String,
    next_node_id: String,
    next_opportunity_node_id: String,
    next_stage_summary: String,
    next_opportunity_kind: String,
    next_outcome_summary: String,
    next_feedback_focus: String,
    next_opportunity_hint: String,
    next_opportunity_playbook: String,
    next_opportunity_command: String,
    next_opportunity_target: WorldRouteStoryOpportunityTarget,
}

impl WorldRouteStoryView {
    fn from_route_data(
        route_preview: &Value,
        route_task_graph: &Value,
        task_views: &[WorldRouteTaskGraphView],
    ) -> Self {
        let preview_items = route_preview
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let preview_item_count = route_preview
            .get("item_count")
            .and_then(Value::as_u64)
            .unwrap_or(preview_items.len() as u64);
        let task_linked_count = route_preview
            .get("task_linked_count")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| {
                preview_items
                    .iter()
                    .filter(|item| {
                        item.get("task_id")
                            .and_then(Value::as_str)
                            .map(|task_id| !task_id.trim().is_empty())
                            .unwrap_or(false)
                    })
                    .count() as u64
            });
        let task_graph_count = route_task_graph
            .get("task_count")
            .and_then(Value::as_u64)
            .unwrap_or(task_views.len() as u64);
        let fallback_location_id = preview_items
            .iter()
            .find_map(|item| item.get("location_id").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();
        let fallback_summary = preview_items
            .iter()
            .find_map(|item| item.get("summary").and_then(Value::as_str))
            .unwrap_or("路线摘要整理中。")
            .to_string();
        let fallback_target = world_route_command_target("/world action 继续推进下一步机会。");
        if let Some(task) = task_views.first() {
            return Self {
                preview_item_count,
                task_linked_count,
                task_graph_count,
                next_task_id: task.task_id.clone(),
                next_action_label: task.suggested_action_label.clone(),
                next_panel_id: task.suggested_panel_id.clone(),
                next_command_hint: task.suggested_matrix_command.clone(),
                next_location_id: task.latest_location_id.clone(),
                next_node_id: task.suggested_node_id.clone(),
                next_opportunity_node_id: task.next_opportunity_node_id.clone(),
                next_stage_summary: task.route_stage_summary.clone(),
                next_opportunity_kind: task.next_opportunity_kind.clone(),
                next_outcome_summary: task.outcome_summary.clone(),
                next_feedback_focus: task.feedback_focus.clone(),
                next_opportunity_hint: task.next_opportunity_hint.clone(),
                next_opportunity_playbook: task.next_opportunity_playbook.clone(),
                next_opportunity_command: task.next_opportunity_command.clone(),
                next_opportunity_target: WorldRouteStoryOpportunityTarget {
                    action_label: task.next_opportunity_action_label.clone(),
                    panel_id: task.next_opportunity_panel_id.clone(),
                    input_id: task.next_opportunity_input_id.clone(),
                    input_value: task.next_opportunity_input_value.clone(),
                    textarea_id: task.next_opportunity_textarea_id.clone(),
                    body: task.opportunity_body().to_string(),
                    node_id: task.next_opportunity_node_id.clone(),
                },
            };
        }
        Self {
            preview_item_count,
            task_linked_count,
            task_graph_count,
            next_task_id: String::new(),
            next_action_label: fallback_target.action_label.clone(),
            next_panel_id: fallback_target.panel_id.clone(),
            next_command_hint: "/world action 继续推进下一步机会。".to_string(),
            next_location_id: fallback_location_id,
            next_node_id: String::new(),
            next_opportunity_node_id: String::new(),
            next_stage_summary: fallback_summary,
            next_opportunity_kind: "contract_capture".to_string(),
            next_outcome_summary: "Outcome summary pending.".to_string(),
            next_feedback_focus: "证据和下一步待补齐。".to_string(),
            next_opportunity_hint: "Opportunity hint pending.".to_string(),
            next_opportunity_playbook: "Opportunity playbook pending.".to_string(),
            next_opportunity_command: "/world action 继续推进下一步机会。".to_string(),
            next_opportunity_target: WorldRouteStoryOpportunityTarget {
                action_label: fallback_target.action_label,
                panel_id: fallback_target.panel_id,
                input_id: fallback_target.input_id,
                input_value: fallback_target.input_value,
                textarea_id: fallback_target.textarea_id,
                body: fallback_target.body,
                node_id: String::new(),
            },
        }
    }

    pub(super) fn to_value(&self) -> Value {
        json!({
            "preview_item_count": self.preview_item_count,
            "task_linked_count": self.task_linked_count,
            "task_graph_count": self.task_graph_count,
            "next_task_id": &self.next_task_id,
            "next_action_label": &self.next_action_label,
            "next_panel_id": &self.next_panel_id,
            "next_command_hint": &self.next_command_hint,
            "next_location_id": &self.next_location_id,
            "next_node_id": &self.next_node_id,
            "next_opportunity_node_id": &self.next_opportunity_node_id,
            "next_stage_summary": &self.next_stage_summary,
            "next_opportunity_kind": &self.next_opportunity_kind,
            "next_outcome_summary": &self.next_outcome_summary,
            "next_feedback_focus": &self.next_feedback_focus,
            "next_opportunity_hint": &self.next_opportunity_hint,
            "next_opportunity_playbook": &self.next_opportunity_playbook,
            "next_opportunity_command": &self.next_opportunity_command,
            "next_opportunity_target": {
                "action_label": &self.next_opportunity_target.action_label,
                "panel_id": &self.next_opportunity_target.panel_id,
                "input_id": &self.next_opportunity_target.input_id,
                "input_value": &self.next_opportunity_target.input_value,
                "textarea_id": &self.next_opportunity_target.textarea_id,
                "body": &self.next_opportunity_target.body,
                "node_id": &self.next_opportunity_target.node_id,
            },
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct WorldRouteArtifacts {
    pub(super) preview: Value,
    pub(super) task_graph: Value,
    pub(super) task_views: Vec<WorldRouteTaskGraphView>,
    pub(super) story: WorldRouteStoryView,
}

#[derive(Debug, Clone)]
struct WorldRoutePreviewItem {
    route_bucket: String,
    location_id: String,
    task_id: String,
    route_status: String,
    created_at_epoch: i64,
    event_id: String,
    contract_id: String,
    completion_id: String,
    purchase_id: String,
    listing_id: String,
    work_order_id: String,
    title: String,
    summary: String,
    detail: String,
}

impl WorldRoutePreviewItem {
    fn from_value(item: &Value) -> Self {
        Self {
            route_bucket: item
                .get("route_bucket")
                .and_then(Value::as_str)
                .unwrap_or("route")
                .to_string(),
            location_id: item
                .get("location_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            task_id: item
                .get("task_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            route_status: item
                .get("route_status")
                .and_then(Value::as_str)
                .unwrap_or("pending")
                .to_string(),
            created_at_epoch: item
                .get("created_at_epoch")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            event_id: item
                .get("event_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            contract_id: item
                .get("contract_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            completion_id: item
                .get("completion_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            purchase_id: item
                .get("purchase_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            listing_id: item
                .get("listing_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            work_order_id: item
                .get("work_order_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            title: item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("路线记录")
                .to_string(),
            summary: item
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("waiting")
                .to_string(),
            detail: item
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("world flow")
                .to_string(),
        }
    }

    fn task_group_id(&self) -> String {
        let candidates = [
            self.task_id.as_str(),
            self.work_order_id.as_str(),
            self.purchase_id.as_str(),
            self.contract_id.as_str(),
            self.listing_id.as_str(),
            self.completion_id.as_str(),
            self.event_id.as_str(),
        ];
        candidates
            .into_iter()
            .find(|value| !value.trim().is_empty())
            .unwrap_or("")
            .to_string()
    }
}

#[derive(Debug, Clone)]
struct WorldRoutePreviewItemView {
    route_bucket: String,
    location_id: String,
    task_id: String,
    title: String,
    summary: String,
    detail: String,
}

impl WorldRoutePreviewItemView {
    fn from_preview_item(item: &WorldRoutePreviewItem) -> Self {
        Self {
            route_bucket: item.route_bucket.clone(),
            location_id: item.location_id.clone(),
            task_id: item.task_id.clone(),
            title: item.title.clone(),
            summary: item.summary.clone(),
            detail: item.detail.clone(),
        }
    }

    fn app_card_html(&self) -> String {
        let detail = if self.detail.is_empty() {
            self.route_bucket.as_str()
        } else {
            self.detail.as_str()
        };
        let summary = if self.summary.is_empty() {
            "waiting"
        } else {
            self.summary.as_str()
        };
        let focus_code = if !self.task_id.is_empty() {
            self.task_id.as_str()
        } else if !self.location_id.is_empty() {
            self.location_id.as_str()
        } else {
            self.route_bucket.as_str()
        };
        format!(
            "<article class=\"module\"><strong>{}</strong><span>{}</span><p>{}</p><div class=\"focus-stack\"><code>{}</code></div></article>",
            escape_world_route_visible_text(&self.title),
            escape_world_route_visible_text(detail),
            escape_world_route_visible_text(summary),
            escape_world_route_visible_text(focus_code),
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct WorldRouteActionButtonView<'a> {
    pub(super) label: &'a str,
    pub(super) panel_id: &'a str,
    pub(super) input_id: &'a str,
    pub(super) input_value: &'a str,
    pub(super) textarea_id: &'a str,
    pub(super) location_id: &'a str,
    pub(super) target_node_id: &'a str,
    pub(super) task_id: &'a str,
    pub(super) contract_id: &'a str,
    pub(super) listing_id: &'a str,
    pub(super) work_order_id: &'a str,
    pub(super) event_id: &'a str,
    pub(super) event_kind: &'a str,
    pub(super) event_body: &'a str,
    pub(super) event_result: &'a str,
    pub(super) event_task_id: &'a str,
    pub(super) body: &'a str,
}

impl WorldRouteActionButtonView<'_> {
    fn target_attrs_html(&self) -> String {
        format!(
            " data-target-panel=\"{}\" data-target-input-id=\"{}\" data-target-value=\"{}\" data-target-textarea-id=\"{}\" data-target-location-id=\"{}\" data-target-node-id=\"{}\" data-target-task-id=\"{}\" data-target-contract-id=\"{}\" data-target-listing-id=\"{}\" data-target-work-order-id=\"{}\" data-target-event-id=\"{}\" data-target-event-kind=\"{}\" data-target-event-body=\"{}\" data-target-event-result=\"{}\" data-target-event-task-id=\"{}\" data-target-body=\"{}\"",
            escape_html_text(self.panel_id),
            escape_html_text(self.input_id),
            escape_html_text(self.input_value),
            escape_html_text(self.textarea_id),
            escape_html_text(self.location_id),
            escape_html_text(self.target_node_id),
            escape_html_text(self.task_id),
            escape_html_text(self.contract_id),
            escape_html_text(self.listing_id),
            escape_html_text(self.work_order_id),
            escape_html_text(self.event_id),
            escape_html_text(self.event_kind),
            escape_html_text(self.event_body),
            escape_html_text(self.event_result),
            escape_html_text(self.event_task_id),
            escape_html_text(self.body),
        )
    }

    pub(super) fn render(&self, class_name: &str) -> String {
        format!(
            "<button type=\"button\" class=\"focus-chip {}\"{}>{}</button>",
            escape_html_text(class_name),
            self.target_attrs_html(),
            escape_world_route_visible_text(self.label),
        )
    }
}

#[derive(Debug, Clone)]
pub(super) struct WorldRouteTaskGraphView {
    pub(super) task_id: String,
    pub(super) latest_bucket: String,
    pub(super) latest_status: String,
    latest_location_id: String,
    event_count: u64,
    contract_count: u64,
    completion_count: u64,
    latest_contract_id: String,
    latest_created_at_epoch: i64,
    route_stage_summary: String,
    pub(super) outcome_summary: String,
    feedback_focus: String,
    next_opportunity_kind: String,
    pub(super) next_opportunity_hint: String,
    next_opportunity_playbook: String,
    next_opportunity_command: String,
    next_opportunity_action_label: String,
    next_opportunity_panel_id: String,
    next_opportunity_input_id: String,
    next_opportunity_input_value: String,
    next_opportunity_textarea_id: String,
    next_opportunity_node_id: String,
    next_opportunity_body: String,
    suggested_action_label: String,
    suggested_panel_id: String,
    suggested_input_id: String,
    suggested_input_value: String,
    suggested_textarea_id: String,
    suggested_matrix_command: String,
    suggested_node_id: String,
    suggested_body: String,
}

impl WorldRouteTaskGraphView {
    fn from_value(task: &Value) -> Self {
        Self {
            task_id: task
                .get("task_id")
                .and_then(Value::as_str)
                .unwrap_or("task")
                .to_string(),
            latest_bucket: task
                .get("latest_bucket")
                .and_then(Value::as_str)
                .unwrap_or("event")
                .to_string(),
            latest_status: task
                .get("latest_status")
                .and_then(Value::as_str)
                .unwrap_or("pending")
                .to_string(),
            latest_location_id: task
                .get("latest_location_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            event_count: task.get("event_count").and_then(Value::as_u64).unwrap_or(0),
            contract_count: task
                .get("contract_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            completion_count: task
                .get("completion_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            latest_contract_id: task
                .get("latest_contract_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            latest_created_at_epoch: task
                .get("latest_created_at_epoch")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            route_stage_summary: task
                .get("route_stage_summary")
                .and_then(Value::as_str)
                .unwrap_or("路线摘要整理中。")
                .to_string(),
            outcome_summary: task
                .get("outcome_summary")
                .and_then(Value::as_str)
                .unwrap_or("结果摘要整理中。")
                .to_string(),
            feedback_focus: task
                .get("feedback_focus")
                .and_then(Value::as_str)
                .unwrap_or("证据和下一步整理中。")
                .to_string(),
            next_opportunity_kind: task
                .get("next_opportunity_kind")
                .and_then(Value::as_str)
                .unwrap_or("contract_capture")
                .to_string(),
            next_opportunity_hint: task
                .get("next_opportunity_hint")
                .and_then(Value::as_str)
                .unwrap_or("Opportunity hint pending.")
                .to_string(),
            next_opportunity_playbook: task
                .get("next_opportunity_playbook")
                .and_then(Value::as_str)
                .unwrap_or("Opportunity playbook pending.")
                .to_string(),
            next_opportunity_command: task
                .get("next_opportunity_command")
                .and_then(Value::as_str)
                .unwrap_or("/world action 继续推进下一步机会。")
                .to_string(),
            next_opportunity_action_label: task
                .get("next_opportunity_action_label")
                .and_then(Value::as_str)
                .unwrap_or("推进下一条支线")
                .to_string(),
            next_opportunity_panel_id: task
                .get("next_opportunity_panel_id")
                .and_then(Value::as_str)
                .unwrap_or(WORLD_ROUTE_ACTION_PANEL_ID)
                .to_string(),
            next_opportunity_input_id: task
                .get("next_opportunity_input_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            next_opportunity_input_value: task
                .get("next_opportunity_input_value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            next_opportunity_textarea_id: task
                .get("next_opportunity_textarea_id")
                .and_then(Value::as_str)
                .unwrap_or(WORLD_ROUTE_ACTION_TEXTAREA_ID)
                .to_string(),
            next_opportunity_node_id: task
                .get("next_opportunity_node_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            next_opportunity_body: task
                .get("next_opportunity_body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            suggested_action_label: task
                .get("suggested_action_label")
                .and_then(Value::as_str)
                .unwrap_or("起草任务后续")
                .to_string(),
            suggested_panel_id: task
                .get("suggested_panel_id")
                .and_then(Value::as_str)
                .unwrap_or(WORLD_ROUTE_ACTION_PANEL_ID)
                .to_string(),
            suggested_input_id: task
                .get("suggested_input_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            suggested_input_value: task
                .get("suggested_input_value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            suggested_textarea_id: task
                .get("suggested_textarea_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            suggested_matrix_command: task
                .get("suggested_matrix_command")
                .and_then(Value::as_str)
                .unwrap_or("/world action 跟进当前任务并记录证据、阻塞和下一步。")
                .to_string(),
            suggested_node_id: task
                .get("suggested_node_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            suggested_body: task
                .get("suggested_body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }
    }

    fn resolved_suggested_input_id(&self) -> &str {
        if !self.suggested_input_id.is_empty() {
            &self.suggested_input_id
        } else if self.suggested_panel_id == WORLD_ROUTE_CONTRACTS_PANEL_ID {
            WORLD_ROUTE_CONTRACT_INPUT_ID
        } else {
            ""
        }
    }

    fn resolved_suggested_input_value(&self) -> &str {
        if !self.suggested_input_value.is_empty() {
            &self.suggested_input_value
        } else if self.resolved_suggested_input_id() == WORLD_ROUTE_CONTRACT_INPUT_ID {
            &self.latest_contract_id
        } else {
            ""
        }
    }

    fn resolved_suggested_textarea_id(&self) -> &str {
        if !self.suggested_textarea_id.is_empty() {
            &self.suggested_textarea_id
        } else if self.suggested_panel_id == WORLD_ROUTE_CONTRACTS_PANEL_ID {
            WORLD_ROUTE_CONTRACT_TEXTAREA_ID
        } else {
            WORLD_ROUTE_ACTION_TEXTAREA_ID
        }
    }

    fn opportunity_body(&self) -> &str {
        if self.next_opportunity_body.is_empty() {
            &self.next_opportunity_command
        } else {
            &self.next_opportunity_body
        }
    }

    fn suggested_action_button_html(&self, class_name: &str) -> String {
        WorldRouteActionButtonView {
            label: &self.suggested_action_label,
            panel_id: &self.suggested_panel_id,
            input_id: self.resolved_suggested_input_id(),
            input_value: self.resolved_suggested_input_value(),
            textarea_id: self.resolved_suggested_textarea_id(),
            location_id: &self.latest_location_id,
            target_node_id: &self.suggested_node_id,
            task_id: &self.task_id,
            contract_id: &self.latest_contract_id,
            listing_id: "",
            work_order_id: "",
            event_id: "",
            event_kind: "",
            event_body: "",
            event_result: "",
            event_task_id: "",
            body: &self.suggested_body,
        }
        .render(class_name)
    }

    fn opportunity_action_button_html(&self, class_name: &str) -> String {
        WorldRouteActionButtonView {
            label: &self.next_opportunity_action_label,
            panel_id: &self.next_opportunity_panel_id,
            input_id: &self.next_opportunity_input_id,
            input_value: &self.next_opportunity_input_value,
            textarea_id: &self.next_opportunity_textarea_id,
            location_id: &self.latest_location_id,
            target_node_id: &self.next_opportunity_node_id,
            task_id: &self.task_id,
            contract_id: "",
            listing_id: "",
            work_order_id: "",
            event_id: "",
            event_kind: "",
            event_body: "",
            event_result: "",
            event_task_id: "",
            body: self.opportunity_body(),
        }
        .render(class_name)
    }

    fn app_card_html(&self) -> String {
        let suggested_action =
            self.suggested_action_button_html("trillionnium-app-route-flow-action");
        let opportunity_action =
            self.opportunity_action_button_html("trillionnium-app-route-flow-action");
        format!(
            "<article class=\"module app-route-task-graph-item\" data-task-id=\"{}\" data-location-id=\"{}\"><strong>{}</strong><span>{} · {} · branch {}</span><p>{} events · {} commissions · {} battle reports</p><p>{}</p><p><strong>Next branch</strong> · {}</p><p>{}</p><div class=\"focus-stack\"><code>{}</code></div><div class=\"focus-stack\">{}{}</div></article>",
            escape_html_text(&self.task_id),
            escape_html_text(&self.latest_location_id),
            escape_html_text(&self.task_id),
            escape_world_route_visible_text(&self.latest_bucket),
            escape_world_route_visible_text(&self.latest_status),
            escape_world_route_visible_text(&self.next_opportunity_kind),
            self.event_count,
            self.contract_count,
            self.completion_count,
            escape_world_route_visible_text(&self.outcome_summary),
            escape_world_route_visible_text(&self.next_opportunity_hint),
            escape_world_route_visible_text(&self.next_opportunity_playbook),
            escape_world_route_visible_text(&self.next_opportunity_command),
            suggested_action,
            opportunity_action,
        )
    }

    pub(super) fn world_flow_card_html(&self) -> String {
        let suggested_action = self.suggested_action_button_html("trillionnium-route-flow-action");
        let opportunity_action =
            self.opportunity_action_button_html("trillionnium-route-flow-action");
        format!(
            "<article class=\"mini task-graph\" data-task-id=\"{}\" data-location-id=\"{}\"><strong>{}</strong><span>{} · {} · branch {}</span><code>{}</code><small>{} events · {} commissions · {} battle reports</small><small>{}</small><small><strong>Next branch</strong> · {}</small><div class=\"focus-stack\"><code>{}</code></div><div class=\"focus-stack\">{}{}</div></article>",
            escape_html_text(&self.task_id),
            escape_html_text(&self.latest_location_id),
            escape_html_text(&self.task_id),
            escape_world_route_visible_text(&self.latest_bucket),
            escape_world_route_visible_text(&self.latest_status),
            escape_world_route_visible_text(&self.next_opportunity_kind),
            escape_html_text(&self.task_id),
            self.event_count,
            self.contract_count,
            self.completion_count,
            escape_world_route_visible_text(&self.outcome_summary),
            escape_world_route_visible_text(&self.next_opportunity_hint),
            escape_world_route_visible_text(&self.next_opportunity_command),
            suggested_action,
            opportunity_action,
        )
    }

    pub(super) fn to_feed_item(&self) -> Value {
        json!({
            "feed_kind": "route_task",
            "source": "route_task_graph",
            "task_id": &self.task_id,
            "location_id": &self.latest_location_id,
            "title": format!("Task {}", &self.task_id),
            "summary": &self.route_stage_summary,
            "detail": format!("latest {}/{}", &self.latest_bucket, &self.latest_status),
            "event_count": self.event_count,
            "contract_count": self.contract_count,
            "completion_count": self.completion_count,
            "latest_contract_id": &self.latest_contract_id,
            "feedback_focus": &self.feedback_focus,
            "next_opportunity_kind": &self.next_opportunity_kind,
            "next_opportunity_hint": &self.next_opportunity_hint,
            "next_opportunity_playbook": &self.next_opportunity_playbook,
            "next_opportunity_command": &self.next_opportunity_command,
            "next_opportunity_action_label": &self.next_opportunity_action_label,
            "next_opportunity_panel_id": &self.next_opportunity_panel_id,
            "next_opportunity_input_id": &self.next_opportunity_input_id,
            "next_opportunity_input_value": &self.next_opportunity_input_value,
            "next_opportunity_textarea_id": &self.next_opportunity_textarea_id,
            "next_opportunity_node_id": &self.next_opportunity_node_id,
            "next_opportunity_body": &self.next_opportunity_body,
            "suggested_action_label": &self.suggested_action_label,
            "suggested_panel_id": &self.suggested_panel_id,
            "suggested_matrix_command": &self.suggested_matrix_command,
            "suggested_node_id": &self.suggested_node_id,
            "suggested_body": &self.suggested_body,
            "created_at_epoch": self.latest_created_at_epoch,
        })
    }
}

pub(super) fn world_route_task_graph_views(
    route_task_graph: &Value,
    limit: usize,
) -> Vec<WorldRouteTaskGraphView> {
    route_task_graph
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(limit)
        .map(|task| WorldRouteTaskGraphView::from_value(&task))
        .collect()
}

fn world_route_preview_items(route_preview: &Value) -> Vec<WorldRoutePreviewItem> {
    route_preview
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| WorldRoutePreviewItem::from_value(&item))
        .collect()
}

fn world_route_preview_item_views(
    route_preview: &Value,
    limit: usize,
) -> Vec<WorldRoutePreviewItemView> {
    world_route_preview_items(route_preview)
        .into_iter()
        .take(limit)
        .map(|item| WorldRoutePreviewItemView::from_preview_item(&item))
        .collect()
}

#[derive(Debug, Clone)]
pub(super) struct ClientAppRouteSurfaceView {
    preview_items: Vec<WorldRoutePreviewItemView>,
    task_graph_items: Vec<WorldRouteTaskGraphView>,
}

impl ClientAppRouteSurfaceView {
    pub(super) fn from_map_hub(map_hub: Option<&Value>) -> Self {
        let preview_items = map_hub
            .and_then(|hub| hub.get("route_preview"))
            .map(|preview| world_route_preview_item_views(preview, 8))
            .unwrap_or_default();
        let task_graph_items = map_hub
            .and_then(|hub| hub.get("route_task_graph"))
            .map(|graph| world_route_task_graph_views(graph, 6))
            .unwrap_or_default();
        Self {
            preview_items,
            task_graph_items,
        }
    }

    pub(super) fn preview_cards_html(&self) -> String {
        self.preview_items
            .iter()
            .map(WorldRoutePreviewItemView::app_card_html)
            .collect::<Vec<_>>()
            .join("")
    }

    pub(super) fn task_graph_cards_html(&self) -> String {
        self.task_graph_items
            .iter()
            .map(WorldRouteTaskGraphView::app_card_html)
            .collect::<Vec<_>>()
            .join("")
    }
}

pub(super) fn build_world_route_artifacts(world: &WorldState) -> WorldRouteArtifacts {
    WorldRouteProjectionContext::new(world).artifacts()
}

// 单所有者串行处理账号事件和管理指令，取码与发送均有时限。
use crate::ledger::{Account, Incident, Ledger, Receipt, Store};
use crate::{
    AccountState, Command, Delivery, Incoming, Observation, Outgoing, QrReply, ReloginPort,
};
use ncd_domain::{SnowLumaQrLoginState, relogin::ReloginSettings};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    time::Duration,
};

pub struct ReloginManager {
    settings: ReloginSettings,
    messengers: Vec<String>,
    state: Ledger,
    store: Store,
    cached: BTreeMap<String, (i64, QrReply)>,
    groups: Vec<u64>,
    notify_recovered: bool,
    failed: bool,
}

impl ReloginManager {
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let (store, mut state) = Store::load(path)?;
        // 停机时间不能算作持续在线或离线的确认窗口。
        for account in state.accounts.values_mut() {
            account.offline_since = None;
            account.online_since = None;
        }
        Ok(Self {
            settings: Default::default(),
            messengers: vec![],
            state,
            store,
            cached: BTreeMap::new(),
            groups: vec![],
            notify_recovered: false,
            failed: false,
        })
    }

    pub fn delivery_options(&mut self, groups: Vec<u64>, notify_recovered: bool) {
        self.groups = groups.into_iter().filter(|g| *g > 0).take(32).collect();
        self.notify_recovered = notify_recovered;
    }

    fn persist(&mut self) -> Result<(), String> {
        if self.failed {
            return Err("重登记录不可用，请修复后重启 watch".into());
        }
        if let Err(error) = self.store.save(&self.state) {
            self.failed = true;
            self.cached.clear();
            return Err(error);
        }
        Ok(())
    }

    pub fn configure(&mut self, mut settings: ReloginSettings, messengers: Vec<String>) {
        settings.normalize();
        self.settings = settings;
        self.messengers = messengers.into_iter().take(64).collect();
        self.state
            .accounts
            .retain(|id, _| self.settings.monitors(id));
        self.cached.retain(|id, _| self.settings.monitors(id));
    }

    pub async fn tick(
        &mut self,
        observations: &[Observation],
        now: i64,
        port: &dyn ReloginPort,
    ) -> Result<(), String> {
        if self.failed {
            return Err("重登记录不可用".into());
        }
        self.state
            .commands
            .retain(|_, at| now.saturating_sub(*at) <= 600 && *at <= now.saturating_add(15));
        self.cached
            .retain(|_, (at, _)| now.saturating_sub(*at) < 30 && *at <= now);
        let valid: HashSet<_> = observations.iter().map(|o| o.bot_id.as_str()).collect();
        self.state
            .accounts
            .retain(|id, _| valid.contains(id.as_str()) && self.settings.monitors(id));
        let mut recovered = Vec::new();
        let mut ready = Vec::new();
        for observation in observations.iter().take(256) {
            let id = &observation.bot_id;
            if !self.settings.monitors(id) {
                continue;
            }
            let account = self.state.accounts.entry(id.clone()).or_default();
            if !observation.reset_generation.is_empty() {
                if account.reset_generation != observation.reset_generation {
                    *account = Account::default();
                    self.cached.remove(id);
                }
                account
                    .reset_generation
                    .clone_from(&observation.reset_generation);
            }
            account.name = observation.name.clone();
            if account.generation != observation.generation {
                self.cached.remove(id);
            }
            account.generation = observation.generation.clone();
            match observation.state {
                AccountState::Online => {
                    self.cached.remove(id);
                    account.seen_online = true;
                    account.offline_since = None;
                    let since = account.online_since.get_or_insert(now);
                    if now.saturating_sub(*since) >= 120 && account.incident.is_some() {
                        recovered.push(id.clone());
                    }
                }
                AccountState::Stopped => {
                    self.cached.remove(id);
                    *account = Account {
                        name: observation.name.clone(),
                        reset_generation: account.reset_generation.clone(),
                        ..Default::default()
                    };
                    port.close_qr(id).await;
                }
                AccountState::Unknown => {
                    self.cached.remove(id);
                    account.online_since = None;
                    account.offline_since = None;
                }
                AccountState::Offline if account.seen_online => {
                    account.online_since = None;
                    let since = *account.offline_since.get_or_insert(now);
                    if account.incident.is_none() {
                        account.incident = Some(Incident {
                            id: uuid::Uuid::new_v4().to_string(),
                            started_at: now,
                            ignored: false,
                            deliveries: BTreeMap::new(),
                            participants: BTreeMap::new(),
                            refreshed_at: None,
                            group_deliveries: BTreeMap::new(),
                            fallback_attempted: false,
                            retry_at: 0,
                            refresh_replies: BTreeMap::new(),
                        });
                    }
                    let cooled = account
                        .last_notice
                        .is_none_or(|at| now.saturating_sub(at) >= 600);
                    if now.saturating_sub(since) >= i64::from(self.settings.offline_grace_seconds)
                        && (cooled
                            || account
                                .incident
                                .as_ref()
                                .is_some_and(|i| !i.deliveries.is_empty()))
                    {
                        ready.push(id.clone());
                    }
                }
                AccountState::Offline => {}
            }
        }
        self.persist()?;
        for id in recovered {
            self.recovered(&id, port).await?;
        }
        for id in ready {
            self.notify(&id, now, port).await?;
        }
        Ok(())
    }

    async fn recovered(&mut self, bot: &str, port: &dyn ReloginPort) -> Result<(), String> {
        if !bounded_observe(port, bot).await.is_some_and(|observation| {
            observation.state == AccountState::Online && self.same_incident(&observation)
        }) {
            return Ok(());
        }
        let Some(account) = self.state.accounts.get_mut(bot) else {
            return Ok(());
        };
        let Some(incident) = account.incident.take() else {
            return Ok(());
        };
        let text = format!("{}（{bot}）已恢复。", account.name);
        self.persist()?;
        port.close_qr(bot).await;
        let mut recipients = incident.participants;
        if self.notify_recovered {
            for (recipient, receipt) in incident.deliveries {
                if receipt.message_id.is_some() {
                    recipients.entry(recipient).or_insert(receipt.messenger);
                }
            }
        }
        for (recipient, messenger) in recipients {
            if self.settings.administrator_ids.contains(&recipient)
                && port.authorized(bot, Some(recipient)).await
            {
                let _ = bounded_send(
                    port,
                    &messenger,
                    recipient,
                    Outgoing {
                        text: text.clone(),
                        image: None,
                    },
                )
                .await;
            }
        }
        Ok(())
    }

    async fn notify(&mut self, bot: &str, now: i64, port: &dyn ReloginPort) -> Result<(), String> {
        if !port.authorized(bot, None).await {
            return Ok(());
        }
        let Some(account) = self.state.accounts.get(bot) else {
            return Ok(());
        };
        let Some(incident) = account.incident.as_ref().filter(|i| !i.ignored) else {
            return Ok(());
        };
        if now < incident.retry_at {
            return Ok(());
        }
        let groups: Vec<_> = self
            .groups
            .iter()
            .copied()
            .filter(|g| !incident.group_deliveries.contains_key(g))
            .collect();
        let recipients: Vec<_> = self
            .settings
            .administrator_ids
            .iter()
            .copied()
            .filter(|id| !incident.deliveries.contains_key(id))
            .collect();
        if recipients.is_empty() && groups.is_empty() {
            return Ok(());
        }
        let name = account.name.clone();
        // 无在线代发方时不触碰 QQ 登录页面。
        let candidates = self.candidates(bot, port).await;
        if candidates.is_empty() {
            self.fallback(bot, port).await?;
            return Ok(());
        }
        if let Some(incident) = self.incident_mut(bot) {
            incident.retry_at = now.saturating_add(60);
        }
        self.persist()?;
        let qr = self.capture(bot, false, now, port).await;
        for recipient in recipients {
            if !self.still_offline(bot, port).await || !port.authorized(bot, Some(recipient)).await
            {
                break;
            }
            for messenger in &candidates {
                if !self.still_offline(bot, port).await
                    || !port.authorized(bot, Some(recipient)).await
                {
                    break;
                }
                // 上一个发送方明确拒绝时也要重读，换路期间用户可能已经扫码。
                let image = self.current_image(bot, qr.as_ref(), port).await;
                let text = format!(
                    "{name}（{bot}）掉线。{}\n回复「刷新二维码」或发送「/ncd 刷新 {bot}」。",
                    if image.is_some() {
                        "需要重新登录，请扫码。"
                    } else {
                        "暂时无法取得二维码。"
                    }
                );
                if let Some(account) = self.state.accounts.get_mut(bot) {
                    account.last_notice = Some(now);
                    if let Some(incident) = &mut account.incident {
                        incident.deliveries.insert(
                            recipient,
                            Receipt {
                                messenger: messenger.clone(),
                                message_id: None,
                            },
                        );
                    }
                }
                self.persist()?;
                match bounded_send(
                    port,
                    messenger,
                    recipient,
                    Outgoing {
                        text: text.clone(),
                        image: image.clone(),
                    },
                )
                .await
                {
                    Delivery::Sent { message_id } => {
                        if let Some(receipt) = self
                            .incident_mut(bot)
                            .and_then(|i| i.deliveries.get_mut(&recipient))
                        {
                            receipt.message_id = Some(message_id);
                        }
                        self.persist()?;
                        break;
                    }
                    Delivery::Unknown => break,
                    Delivery::Rejected => {
                        if let Some(incident) = self.incident_mut(bot) {
                            incident.deliveries.remove(&recipient);
                        }
                        self.persist()?;
                    }
                }
            }
        }
        for group in groups {
            if !self.still_offline(bot, port).await || !port.authorized(bot, None).await {
                break;
            }
            for messenger in &candidates {
                if let Some(incident) = self.incident_mut(bot) {
                    incident.group_deliveries.insert(
                        group,
                        Receipt {
                            messenger: messenger.clone(),
                            message_id: None,
                        },
                    );
                }
                self.persist()?;
                let delivery = tokio::time::timeout(Duration::from_secs(12), port.send_group(messenger, group,
                    format!("{name}（{bot}）掉线。管理员可私聊发送「/ncd 刷新 {bot}」获取重登二维码。")))
                    .await.unwrap_or(Delivery::Unknown);
                match delivery {
                    Delivery::Rejected => {
                        if let Some(i) = self.incident_mut(bot) {
                            i.group_deliveries.remove(&group);
                        }
                        self.persist()?;
                    }
                    Delivery::Unknown => break,
                    Delivery::Sent { message_id } => {
                        if let Some(receipt) = self
                            .incident_mut(bot)
                            .and_then(|i| i.group_deliveries.get_mut(&group))
                        {
                            receipt.message_id = Some(message_id);
                        }
                        self.persist()?;
                        break;
                    }
                }
            }
        }
        if self
            .state
            .accounts
            .get(bot)
            .and_then(|a| a.incident.as_ref())
            .is_some_and(|i| i.deliveries.is_empty() && i.group_deliveries.is_empty())
        {
            self.fallback(bot, port).await?;
        }
        Ok(())
    }

    async fn fallback(&mut self, bot: &str, port: &dyn ReloginPort) -> Result<(), String> {
        if let Some(incident) = self.incident_mut(bot) {
            if incident.fallback_attempted {
                return Ok(());
            }
            incident.fallback_attempted = true;
        }
        self.persist()?;
        if self.still_offline(bot, port).await && port.authorized(bot, None).await {
            let _ = tokio::time::timeout(Duration::from_secs(15), port.fallback(bot)).await;
        }
        Ok(())
    }

    async fn current_image(
        &self,
        bot: &str,
        qr: Option<&QrReply>,
        port: &dyn ReloginPort,
    ) -> Option<String> {
        let qr = qr?;
        let current = bounded_observe(port, bot).await?;
        if !self.same_incident(&current)
            || current.state != AccountState::Offline
            || current.generation != qr.generation
        {
            return None;
        }
        if !tokio::time::timeout(Duration::from_secs(15), port.qr_is_current(bot, qr))
            .await
            .unwrap_or(false)
        {
            return None;
        }
        qr.image()
    }

    fn incident_mut(&mut self, bot: &str) -> Option<&mut Incident> {
        self.state.accounts.get_mut(bot)?.incident.as_mut()
    }

    async fn candidates(&self, bot: &str, port: &dyn ReloginPort) -> Vec<String> {
        let mut candidates = Vec::new();
        for id in &self.messengers {
            if id != bot
                && tokio::time::timeout(Duration::from_secs(8), port.can_send(id))
                    .await
                    .unwrap_or(false)
            {
                candidates.push(id.clone());
            }
        }
        if let Some(sticky) = self
            .state
            .accounts
            .get(bot)
            .and_then(|a| a.incident.as_ref())
            .and_then(|i| i.deliveries.values().next())
            .map(|r| &r.messenger)
        {
            if let Some(index) = candidates.iter().position(|id| id == sticky) {
                candidates.swap(0, index);
            }
        }
        candidates
    }

    async fn still_offline(&self, bot: &str, port: &dyn ReloginPort) -> bool {
        bounded_observe(port, bot)
            .await
            .is_some_and(|o| o.state == AccountState::Offline && self.same_incident(&o))
    }

    fn same_incident(&self, observation: &Observation) -> bool {
        self.state
            .accounts
            .get(&observation.bot_id)
            .is_some_and(|account| observation.reset_generation == account.reset_generation)
    }

    async fn capture(
        &mut self,
        bot: &str,
        refresh: bool,
        now: i64,
        port: &dyn ReloginPort,
    ) -> Option<QrReply> {
        let before = bounded_observe(port, bot).await?;
        if before.state != AccountState::Offline || !self.same_incident(&before) {
            return None;
        }
        let reply = tokio::time::timeout(Duration::from_secs(15), port.qr(bot, refresh))
            .await
            .ok()?
            .ok()?;
        let after = bounded_observe(port, bot).await?;
        if after.state != AccountState::Offline
            || !self.same_incident(&after)
            || before.generation != after.generation
            || reply.generation != after.generation
        {
            self.cached.remove(bot);
            return None;
        }
        if reply.image().is_some() {
            self.cached.insert(bot.to_owned(), (now, reply.clone()));
        } else {
            self.cached.remove(bot);
        }
        Some(reply)
    }

    pub async fn command(
        &mut self,
        incoming: Incoming,
        now: i64,
        port: &dyn ReloginPort,
    ) -> Result<(), String> {
        if self.failed {
            return Err("重登记录不可用".into());
        }
        if !self.settings.enabled
            || !self.settings.administrator_ids.contains(&incoming.sender)
            || !self.messengers.contains(&incoming.messenger)
            || now.saturating_sub(incoming.at) > 90
            || incoming.at > now.saturating_add(15)
        {
            return Ok(());
        }
        if !port.authorized("", Some(incoming.sender)).await {
            return Ok(());
        }
        let key = format!(
            "{}:{}:{}",
            incoming.messenger, incoming.sender, incoming.message_id
        );
        if self.state.commands.contains_key(&key) || self.state.commands.len() >= 2048 {
            return Ok(());
        }
        let prefix = format!("{}:{}:", incoming.messenger, incoming.sender);
        if self
            .state
            .commands
            .iter()
            .any(|(key, at)| key.starts_with(&prefix) && now.saturating_sub(*at) < 3)
        {
            return Ok(());
        }
        self.state.commands.insert(key, now);
        self.persist()?;
        let targets: Vec<_> = self
            .state
            .accounts
            .iter()
            .filter(|(_, a)| a.incident.is_some())
            .map(|(id, a)| (id.clone(), a.name.clone()))
            .collect();
        if matches!(incoming.command, Command::Status) {
            let text = if targets.is_empty() {
                "没有待处理的掉线账号。".into()
            } else {
                targets
                    .iter()
                    .map(|(id, name)| format!("{name}（{id}）"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            reply_text(port, &incoming, text).await;
            return Ok(());
        }
        let target = match &incoming.command {
            Command::Refresh(Some(id)) | Command::Ignore(id) => {
                let found: Vec<_> = targets
                    .iter()
                    .filter(|(bid, name)| bid == id || name == id)
                    .collect();
                (found.len() == 1).then(|| found[0].0.clone())
            }
            Command::Refresh(None) => {
                let found: Vec<_> = targets
                    .iter()
                    .filter(|(id, _)| {
                        self.state
                            .accounts
                            .get(id)
                            .and_then(|a| a.incident.as_ref())
                            .and_then(|i| i.deliveries.get(&incoming.sender))
                            .is_some_and(|r| {
                                r.messenger == incoming.messenger
                                    && incoming
                                        .reply_to
                                        .as_ref()
                                        .is_none_or(|reply| r.message_id.as_ref() == Some(reply))
                            })
                    })
                    .collect();
                (found.len() == 1).then(|| found[0].0.clone())
            }
            _ => None,
        };
        let Some(bot) = target else {
            reply_text(
                port,
                &incoming,
                "请用「/ncd 状态」查看账号，再发送「/ncd 刷新 QQ号」。".into(),
            )
            .await;
            return Ok(());
        };
        if !port.authorized(&bot, Some(incoming.sender)).await {
            return Ok(());
        }
        if matches!(incoming.command, Command::Ignore(_)) {
            if let Some(incident) = self.incident_mut(&bot) {
                incident.ignored = true;
            }
            self.persist()?;
            self.cached.remove(&bot);
            port.close_qr(&bot).await;
            reply_text(port, &incoming, format!("已忽略 {bot} 的本次掉线。")).await;
            return Ok(());
        }
        if !self.still_offline(&bot, port).await {
            reply_text(
                port,
                &incoming,
                "账号已在线或当前无法确认登录状态，未刷新二维码。".into(),
            )
            .await;
            return Ok(());
        }
        let cooldown = i64::from(self.settings.refresh_cooldown_seconds);
        let recent = self
            .state
            .accounts
            .get(&bot)
            .and_then(|a| a.incident.as_ref())
            .and_then(|i| i.refreshed_at)
            .is_some_and(|at| now.saturating_sub(at) < cooldown);
        // 同一个二维码窗口内不反复回复，也不会因另一个管理员刷新而破坏手机确认。
        if recent {
            let already_replied = self
                .incident_mut(&bot)
                .is_some_and(|i| i.refresh_replies.contains_key(&incoming.sender));
            if already_replied {
                return Ok(());
            }
            let cached = self
                .cached
                .get(&bot)
                .filter(|(at, _)| now.saturating_sub(*at) < 30)
                .map(|(_, qr)| qr.clone());
            if let Some(image) = self.current_image(&bot, cached.as_ref(), port).await {
                if let Some(i) = self.incident_mut(&bot) {
                    i.refresh_replies.insert(incoming.sender, now);
                    i.participants
                        .insert(incoming.sender, incoming.messenger.clone());
                }
                self.persist()?;
                if port.authorized(&bot, Some(incoming.sender)).await {
                    let _ = bounded_send(
                        port,
                        &incoming.messenger,
                        incoming.sender,
                        Outgoing {
                            text: format!("{bot} 的当前二维码："),
                            image: Some(image),
                        },
                    )
                    .await;
                }
            }
            return Ok(());
        }
        if let Some(incident) = self.incident_mut(&bot) {
            incident.refreshed_at = Some(now);
            incident.refresh_replies.clear();
            incident.refresh_replies.insert(incoming.sender, now);
            incident
                .participants
                .insert(incoming.sender, incoming.messenger.clone());
        }
        self.persist()?;
        let result = self.capture(&bot, true, now, port).await;
        let image = self.current_image(&bot, result.as_ref(), port).await;
        let text = match result.as_ref().map(|r| r.snapshot.state) {
            Some(SnowLumaQrLoginState::Scanned | SnowLumaQrLoginState::LoginDetected) => {
                "已扫码，请在手机上确认。".into()
            }
            _ if image.is_some() => format!("{bot} 的重登二维码："),
            _ => "暂时无法取得二维码，请通过 QQ / VNC 检查登录页面。".into(),
        };
        if self.still_offline(&bot, port).await
            && port.authorized(&bot, Some(incoming.sender)).await
        {
            let _ = bounded_send(
                port,
                &incoming.messenger,
                incoming.sender,
                Outgoing { text, image },
            )
            .await;
        }
        Ok(())
    }
}

async fn bounded_observe(port: &dyn ReloginPort, bot: &str) -> Option<Observation> {
    tokio::time::timeout(Duration::from_secs(8), port.observe(bot))
        .await
        .ok()
        .flatten()
}

async fn bounded_send(
    port: &dyn ReloginPort,
    messenger: &str,
    recipient: u64,
    message: Outgoing,
) -> Delivery {
    tokio::time::timeout(
        Duration::from_secs(12),
        port.send(messenger, recipient, message),
    )
    .await
    .unwrap_or(Delivery::Unknown)
}

async fn reply_text(port: &dyn ReloginPort, incoming: &Incoming, text: String) {
    let _ = bounded_send(
        port,
        &incoming.messenger,
        incoming.sender,
        Outgoing { text, image: None },
    )
    .await;
}

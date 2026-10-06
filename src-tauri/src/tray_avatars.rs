// 原生托盘面板的头像槽：图片留在主线程（Rc），加载在 async runtime，结果经 run_on_main_thread 送回。

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::avatar_cache::{self, AvatarKey, AvatarPixels};
use crate::native_panel::gfx::Image;

// 在途太久（网络卡住、送回时面板正借着）就当没发过，下次打开再要一次
const LOADING_STALE: Duration = Duration::from_secs(30);
// 失败的过一会儿允许重试；真正的网络冷却在 avatar_cache 里
const FAILED_RETRY: Duration = Duration::from_secs(60);
const MAX_SLOTS: usize = 64;

enum Slot {
    Loading(Instant),
    Ready(Rc<Image>),
    Failed(Instant),
}

#[derive(Default)]
pub struct AvatarSlots {
    slots: HashMap<AvatarKey, Slot>,
}

/// 头像送回主线程后的落点：各面板模块自己找到单例、填槽、重画。
pub type Deliver = fn(AvatarKey, Option<Arc<AvatarPixels>>);

impl AvatarSlots {
    pub fn image(&self, key: &AvatarKey) -> Option<Rc<Image>> {
        match self.slots.get(key) {
            Some(Slot::Ready(image)) => Some(Rc::clone(image)),
            _ => None,
        }
    }

    /// 挑出还要加载的 key 并标成在途；已有图、在途未超时、刚失败的都跳过。
    pub fn claim(&mut self, keys: impl IntoIterator<Item = AvatarKey>) -> Vec<AvatarKey> {
        let now = Instant::now();
        let mut out = Vec::new();
        for key in keys {
            let wanted = match self.slots.get(&key) {
                None => true,
                Some(Slot::Ready(_)) => false,
                Some(Slot::Loading(at)) => now.duration_since(*at) > LOADING_STALE,
                Some(Slot::Failed(at)) => now.duration_since(*at) > FAILED_RETRY,
            };
            if wanted {
                if self.slots.len() >= MAX_SLOTS {
                    self.slots.clear();
                }
                self.slots.insert(key.clone(), Slot::Loading(now));
                out.push(key);
            }
        }
        out
    }

    pub fn fill(&mut self, key: AvatarKey, pixels: Option<Arc<AvatarPixels>>) {
        let slot = match pixels {
            Some(px) => Slot::Ready(Rc::new(Image {
                width: px.size,
                height: px.size,
                rgba: px.rgba.clone(),
            })),
            None => Slot::Failed(Instant::now()),
        };
        self.slots.insert(key, slot);
    }

    /// 只留当前快照还用得到的，长期挂在托盘时不让槽位一直涨。
    pub fn retain(&mut self, keep: &HashSet<AvatarKey>) {
        self.slots.retain(|key, _| keep.contains(key));
    }
}

/// 在 async runtime 上逐个加载，结果送回主线程交给 `deliver`。
pub fn spawn_loads(app: &tauri::AppHandle, keys: Vec<AvatarKey>, deliver: Deliver) {
    for key in keys {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let pixels = avatar_cache::load(&app, &key).await;
            let _ = app.run_on_main_thread(move || deliver(key, pixels));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_skips_ready_and_in_flight() {
        let mut slots = AvatarSlots::default();
        let a = AvatarKey::user("10001").unwrap();
        let b = AvatarKey::user("10002").unwrap();
        assert_eq!(slots.claim([a.clone(), b.clone()]).len(), 2);
        assert!(slots.claim([a.clone()]).is_empty(), "in flight");
        slots.fill(
            a.clone(),
            Some(Arc::new(AvatarPixels {
                size: 2,
                rgba: vec![0; 16],
            })),
        );
        assert!(slots.image(&a).is_some());
        assert!(slots.claim([a.clone()]).is_empty(), "ready");
        slots.fill(b.clone(), None);
        assert!(slots.image(&b).is_none());
        assert!(slots.claim([b.clone()]).is_empty(), "just failed");
        slots.retain(&HashSet::from([a.clone()]));
        assert_eq!(slots.claim([b]).len(), 1, "dropped slot is requested again");
    }
}

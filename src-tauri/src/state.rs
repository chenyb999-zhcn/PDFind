// 搜索任务状态: 取消标志 + 单任务护栏(同时只允许一个搜索)
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct SearchState {
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            cancel: Mutex::new(None),
        }
    }

    // 开始新任务; 已有任务在跑则返回 None
    pub fn begin(&self) -> Option<Arc<AtomicBool>> {
        let mut g = self.cancel.lock().unwrap();
        if g.is_some() {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        *g = Some(flag.clone());
        Some(flag)
    }

    // 任务结束(正常/取消/出错)清出槽位
    pub fn end(&self) {
        self.cancel.lock().unwrap().take();
    }

    // 请求取消当前任务
    pub fn cancel(&self) -> bool {
        match self.cancel.lock().unwrap().as_ref() {
            Some(f) => {
                f.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }
}

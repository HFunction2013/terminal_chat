use once_cell::sync::Lazy;
use std::sync::Mutex;

#[allow(dead_code)]
struct Hook<F> {
    name: &'static str,
    f: F,
}

type BeforeHook = Hook<Box<dyn Fn(&mut String) -> bool + Send + Sync>>;
type OnHook     = Hook<Box<dyn Fn(&str) -> bool + Send + Sync>>;
type AfterHook  = Hook<Box<dyn Fn(&str) -> bool + Send + Sync>>;

static BEFORE_HOOKS: Lazy<Mutex<Vec<BeforeHook>>> =
    Lazy::new(|| Mutex::new(Vec::new()));
static ON_HOOKS: Lazy<Mutex<Vec<OnHook>>> =
    Lazy::new(|| Mutex::new(Vec::new()));
static AFTER_HOOKS: Lazy<Mutex<Vec<AfterHook>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

pub mod internal {
    std::thread_local! {
        pub static IN_PRINT: std::cell::RefCell<bool> =
            const { std::cell::RefCell::new(false) };
    }
}

/* ---------- 注册 API ---------- */

pub fn register_before_print<F>(name: &'static str, f: F)
where
    F: Fn(&mut String) -> bool + Send + Sync + 'static,
{
    if let Ok(mut hooks) = BEFORE_HOOKS.lock() {
        hooks.push(Hook { name, f: Box::new(f) });
    }
}

pub fn register_on_print<F>(name: &'static str, f: F)
where
    F: Fn(&str) -> bool + Send + Sync + 'static,
{
    if let Ok(mut hooks) = ON_HOOKS.lock() {
        hooks.push(Hook { name, f: Box::new(f) });
    }
}

pub fn register_after_print<F>(name: &'static str, f: F)
where
    F: Fn(&str) -> bool + Send + Sync + 'static,
{
    if let Ok(mut hooks) = AFTER_HOOKS.lock() {
        hooks.push(Hook { name, f: Box::new(f) });
    }
}

pub fn clear_hooks() {
    let _ = BEFORE_HOOKS.lock().map(|mut h| h.clear());
    let _ = ON_HOOKS.lock().map(|mut h| h.clear());
    let _ = AFTER_HOOKS.lock().map(|mut h| h.clear());
}

pub fn run_before_chain(content: &mut String) -> bool {
    if let Ok(hooks) = BEFORE_HOOKS.lock() {
        for hook in hooks.iter() {
            if (hook.f)(content) {
                return true;
            }
        }
    }
    false
}

/// 返回 true：被拦截，终止链
pub fn run_on_chain(content: &str) -> bool {
    if let Ok(hooks) = ON_HOOKS.lock() {
        for hook in hooks.iter() {
            if (hook.f)(content) {
                return true; 
            }
        }
    }
    false
}

pub fn run_after_chain(content: &str) -> bool {
    if let Ok(hooks) = AFTER_HOOKS.lock() {
        for hook in hooks.iter() {
            if (hook.f)(content) {
                return true;
            }
        }
    }
    false
}

/* ---------- 宏（✅ 清晰语义） ---------- */

#[macro_export]
macro_rules! print_content {
    ($($arg:tt)*) => {{
        use std::fmt::Write;

        $crate::internal::IN_PRINT.with(|in_print| {
            if *in_print.borrow() {
                return;
            }
            *in_print.borrow_mut() = true;

            let mut content = String::new();
            let _ = write!(&mut content, $($arg)*);

            // before：可修改，可拦截
            if $crate::run_before_chain(&mut content) {
                *in_print.borrow_mut() = false;
                return;
            }

            // on：校验 / 拦截
            if $crate::run_on_chain(&content) {
                *in_print.borrow_mut() = false;
                return;
            }

            println!("{}", content);

            let _ = $crate::run_after_chain(&content);

            *in_print.borrow_mut() = false;
        });
    }};
}
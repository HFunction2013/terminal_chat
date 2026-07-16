#[macro_export]
macro_rules! define_hook_system {
    (
        $fn_name:ident,
        $prefix:literal,
        $before_arg:ty,
        $on_arg:ty,
        $after_arg:ty,
        $func_arg:ty
    ) => {
        use once_cell::sync::Lazy;
        use std::sync::Mutex;
        use $crate::hook::Hook;

        paste::paste! {
            type [<Before $prefix:camel Hook>] = Hook<Box<dyn Fn($before_arg) -> bool + Send + Sync>>;
            type [<On $prefix:camel Hook>] = Hook<Box<dyn Fn($on_arg) -> bool + Send + Sync>>;
            type [<After $prefix:camel Hook>] = Hook<Box<dyn Fn($after_arg) -> bool + Send + Sync>>;

            static [<BEFORE_ $prefix:upper _HOOKS>]: Lazy<Mutex<Vec<[<Before $prefix:camel Hook>]>>> =
                Lazy::new(|| Mutex::new(Vec::new()));
            static [<ON_ $prefix:upper _HOOKS>]: Lazy<Mutex<Vec<[<On $prefix:camel Hook>]>>> =
                Lazy::new(|| Mutex::new(Vec::new()));
            static [<AFTER_ $prefix:upper _HOOKS>]: Lazy<Mutex<Vec<[<After $prefix:camel Hook>]>>> =
                Lazy::new(|| Mutex::new(Vec::new()));

            pub mod internal {
                std::thread_local! {
                    pub static [<IN_ $prefix:upper>]: std::cell::RefCell<bool> =
                        const { std::cell::RefCell::new(false) };
                }
            }

            pub fn [<register_before_ $prefix>]<F>(name: &'static str, f: F)
            where
                F: Fn($before_arg) -> bool + Send + Sync + 'static,
            {
                if let Ok(mut hooks) = [<BEFORE_ $prefix:upper _HOOKS>].lock() {
                    hooks.push(Hook { name, f: Box::new(f) });
                }
            }

            pub fn [<register_on_ $prefix>]<F>(name: &'static str, f: F)
            where
                F: Fn($on_arg) -> bool + Send + Sync + 'static,
            {
                if let Ok(mut hooks) = [<ON_ $prefix:upper _HOOKS>].lock() {
                    hooks.push(Hook { name, f: Box::new(f) });
                }
            }

            pub fn [<register_after_ $prefix>]<F>(name: &'static str, f: F)
            where
                F: Fn($after_arg) -> bool + Send + Sync + 'static,
            {
                if let Ok(mut hooks) = [<AFTER_ $prefix:upper _HOOKS>].lock() {
                    hooks.push(Hook { name, f: Box::new(f) });
                }
            }

            pub fn [<clear_ $prefix _hooks>]() {
                let _ = [<BEFORE_ $prefix:upper _HOOKS>].lock().map(|mut h| h.clear());
                let _ = [<ON_ $prefix:upper _HOOKS>].lock().map(|mut h| h.clear());
                let _ = [<AFTER_ $prefix:upper _HOOKS>].lock().map(|mut h| h.clear());
            }

            pub fn [<run_before_ $prefix _chain>](content: $before_arg) -> bool {
                if let Ok(hooks) = [<BEFORE_ $prefix:upper _HOOKS>].lock() {
                    for hook in hooks.iter() {
                        if (hook.f)(content) {
                            return true;
                        }
                    }
                }
                false
            }

            pub fn [<run_on_ $prefix _chain>](content: $on_arg) -> bool {
                if let Ok(hooks) = [<ON_ $prefix:upper _HOOKS>].lock() {
                    for hook in hooks.iter() {
                        if (hook.f)(content) {
                            return true;
                        }
                    }
                }
                false
            }

            pub fn [<run_after_ $prefix _chain>](content: $after_arg) -> bool {
                if let Ok(hooks) = [<AFTER_ $prefix:upper _HOOKS>].lock() {
                    for hook in hooks.iter() {
                        if (hook.f)(content) {
                            return true;
                        }
                    }
                }
                false
            }

            pub fn [<$prefix:snake>](content: $func_arg) {
                let mut is_reentrant = false;
                internal::[<IN_ $prefix:upper>].with(|flag| {
                    // 修复1：解引用 RefCell 取出 bool
                    if *flag.borrow() {
                        $fn_name(content);
                        is_reentrant = true;
                        return;
                    }
                    flag.replace(true);
                });
                if is_reentrant {
                    return;
                }

                let mut buf = content.to_string();
                if [<run_before_ $prefix _chain>](&mut buf) {
                    internal::[<IN_ $prefix:upper>].with(|flag| flag.replace(false));
                    return;
                }

                if [<run_on_ $prefix _chain>](buf.as_str()) {
                    internal::[<IN_ $prefix:upper>].with(|flag| flag.replace(false));
                    return;
                }

                $fn_name(&buf);
                let _ = [<run_after_ $prefix _chain>](buf.as_str());

                internal::[<IN_ $prefix:upper>].with(|flag| flag.replace(false));
            }
        }
    };
}
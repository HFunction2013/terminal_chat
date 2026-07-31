#[macro_export]
macro_rules! __buf {
    // 修复：解引用再借用，解决双重&mut、Void引用类型不匹配
    (M, $b:ident) => {
        &mut *$b
    };
    (R, $b:ident) => {
        &*$b
    };
    (V, $b:ident) => {
        *$b
    };
}

#[macro_export]
macro_rules! __call {
    (M, $func:ident, $b:ident) => {
        $func(&mut $b)
    };
    (R, $func:ident, $b:ident) => {
        $func(&$b)
    };
    (V, $func:ident, $b:ident) => {
        $func($b.clone())
    };
}

#[macro_export]
macro_rules! define_hook_system {
    (
        $fn:ident,
        $pfx:literal,
        $bm:ident, $om:ident, $am:ident, $cm:ident,
        $bt:ty, $ot:ty, $at:ty, $ct:ty
    ) => {
        paste::paste! {
            type [<Before $pfx:camel Hook>] = $crate::hook::Hook<Box<dyn Fn($bt) -> bool + Send + Sync>>;
            type [<On $pfx:camel Hook>] = $crate::hook::Hook<Box<dyn Fn($ot) -> bool + Send + Sync>>;
            type [<After $pfx:camel Hook>] = $crate::hook::Hook<Box<dyn Fn($at) -> bool + Send + Sync>>;

            static [<BEFORE_ $pfx:upper _HOOKS>]: once_cell::sync::Lazy<std::sync::Mutex<Vec<[<Before $pfx:camel Hook>]>>>
                = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(Vec::new()));
            static [<ON_ $pfx:upper _HOOKS>]: once_cell::sync::Lazy<std::sync::Mutex<Vec<[<On $pfx:camel Hook>]>>>
                = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(Vec::new()));
            static [<AFTER_ $pfx:upper _HOOKS>]: once_cell::sync::Lazy<std::sync::Mutex<Vec<[<After $pfx:camel Hook>]>>>
                = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(Vec::new()));

            static [<IN_ $pfx:upper>]: once_cell::sync::Lazy<std::sync::Mutex<bool>>
                = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(false));

            // 注册API 完全不变
            pub fn [<register_before_ $pfx>]<F>(name: &'static str, f: F)
            where F: Fn($bt) -> bool + Send + Sync + 'static {
                if let Ok(mut hooks) = [<BEFORE_ $pfx:upper _HOOKS>].lock() {
                    hooks.push($crate::hook::Hook { name, f: Box::new(f) });
                }
            }
            pub fn [<register_on_ $pfx>]<F>(name: &'static str, f: F)
            where F: Fn($ot) -> bool + Send + Sync + 'static {
                if let Ok(mut hooks) = [<ON_ $pfx:upper _HOOKS>].lock() {
                    hooks.push($crate::hook::Hook { name, f: Box::new(f) });
                }
            }
            pub fn [<register_after_ $pfx>]<F>(name: &'static str, f: F)
            where F: Fn($at) -> bool + Send + Sync + 'static {
                if let Ok(mut hooks) = [<AFTER_ $pfx:upper _HOOKS>].lock() {
                    hooks.push($crate::hook::Hook { name, f: Box::new(f) });
                }
            }

            pub fn [<clear_ $pfx _hooks>]() {
                let _ = [<BEFORE_ $pfx:upper _HOOKS>].lock().map(|mut h| h.clear());
                let _ = [<ON_ $pfx:upper _HOOKS>].lock().map(|mut h| h.clear());
                let _ = [<AFTER_ $pfx:upper _HOOKS>].lock().map(|mut h| h.clear());
            }

            #[allow(unused_mut)]
            fn [<run_before_ $pfx _chain>](mut buf: &mut $ct) -> bool {
                if let Ok(list) = [<BEFORE_ $pfx:upper _HOOKS>].lock() {
                    for h in list.iter() {
                        if (h.f)($crate::__buf!($bm, buf)) {
                            return true;
                        }
                    }
                }
                false
            }
            fn [<run_on_ $pfx _chain>](buf: &$ct) -> bool {
                if let Ok(list) = [<ON_ $pfx:upper _HOOKS>].lock() {
                    for h in list.iter() {
                        if (h.f)($crate::__buf!($om, buf)) {
                            return true;
                        }
                    }
                }
                false
            }
            fn [<run_after_ $pfx _chain>](buf: &$ct) -> bool {
                if let Ok(list) = [<AFTER_ $pfx:upper _HOOKS>].lock() {
                    for h in list.iter() {
                        if (h.f)($crate::__buf!($am, buf)) {
                            return true;
                        }
                    }
                }
                false
            }

            #[allow(unused_assignments)]
            pub fn [<$pfx:snake>](content: impl Into<$ct>) where $ct: Clone {
                let content = content.into();
                let mut reenter_guard = [<IN_ $pfx:upper>].lock().unwrap();
                let reenter = *reenter_guard;
                if reenter {
                    drop(reenter_guard);
                    let tmp = content.clone();
                    $crate::__call!($cm, $fn, tmp);
                    return;
                }
                *reenter_guard = true;
                drop(reenter_guard);

                let mut interrupted = false;
                match stringify!($bm) {
                    "M" => {
                        let mut buf = content.clone();
                        interrupted = [<run_before_ $pfx _chain>](&mut buf);
                        if !interrupted {
                            interrupted = [<run_on_ $pfx _chain>](&buf);
                        }
                        if !interrupted {
                            $crate::__call!($cm, $fn, buf);
                            let _ = [<run_after_ $pfx _chain>](&buf);
                        }
                    }
                    _ => {
                        let buf = content.clone();
                        interrupted = false;
                        if !interrupted {
                            interrupted = [<run_on_ $pfx _chain>](&buf);
                        }
                        if !interrupted {
                            $crate::__call!($cm, $fn, buf);
                            let _ = [<run_after_ $pfx _chain>](&buf);
                        }
                    }
                }

                *[<IN_ $pfx:upper>].lock().unwrap() = false;
            }
        }
    };
}

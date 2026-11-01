//! Запуск JVM в процессе (invocation API) и единственный JNI-вызов `Bridge.boot`.

use std::ffi::{CString, c_char, c_void};
use std::path::{Path, PathBuf};

const JNI_VERSION_21: i32 = 0x0015_0000;

#[repr(C)]
struct JavaVMOption {
    option_string: *mut c_char,
    extra_info: *mut c_void,
}

#[repr(C)]
struct JavaVMInitArgs {
    version: i32,
    n_options: i32,
    options: *mut JavaVMOption,
    ignore_unrecognized: u8,
}

type CreateJavaVm =
    unsafe extern "system" fn(*mut *mut c_void, *mut *mut c_void, *mut c_void) -> i32;

/// Индексы в таблице функций `JNIEnv` (jni.h).
const FIND_CLASS: usize = 6;
const EXCEPTION_DESCRIBE: usize = 16;
const GET_STATIC_METHOD_ID: usize = 113;
const CALL_STATIC_VOID_METHOD_A: usize = 143;
const EXCEPTION_CHECK: usize = 228;

#[cfg(windows)]
const LIB: &[&str] = &["bin/server/jvm.dll", "jre/bin/server/jvm.dll"];
#[cfg(target_os = "macos")]
const LIB: &[&str] = &["lib/server/libjvm.dylib"];
#[cfg(all(unix, not(target_os = "macos")))]
const LIB: &[&str] = &["lib/server/libjvm.so", "jre/lib/server/libjvm.so"];

/// `jvm.dll`/`libjvm.so` по пути к `java` (или по `PATH`, если путь без каталога).
pub fn find_library(java: &Path) -> Result<PathBuf, String> {
    let exe = if java.components().count() == 1 {
        let name = java.as_os_str();
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
            .flat_map(|d| [d.join(name), d.join(Path::new(name).with_extension("exe"))])
            .find(|p| p.is_file())
            .ok_or_else(|| format!("{} not found in PATH", java.display()))?
    } else {
        java.to_owned()
    };
    let exe = exe.canonicalize().map_or(exe, strip_verbatim);
    // <home>/bin/java → <home>
    let home = exe
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| format!("cannot find Java home for {}", exe.display()))?;
    LIB.iter()
        .map(|l| home.join(l))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("no JVM library in {}", home.display()))
}

/// `\\?\C:\...` от `canonicalize` на Windows → `C:\...`: JVM ищет свой `lib/modules`
/// по пути к `jvm.dll` и с verbatim-префиксом не находит.
fn strip_verbatim(p: PathBuf) -> PathBuf {
    match p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p,
    }
}

/// `chunkwake-api.jar`, все jar из `libs/` и, если передан, jar игры.
///
/// Jar игры нужен ради `net.minecraft.*`: плагины под Paper зовут NMS напрямую
/// (JourneyMap падает на `net/minecraft/network/RegistryFriendlyByteBuf`), а
/// эти классы лежат только во внутреннем jar ванилы. Кладём его в конец
/// classpath: наши реализации Bukkit должны побеждать, если имя совпадёт.
pub fn class_path(api_dir: &Path, game_jar: Option<&Path>) -> Result<String, String> {
    let mut jars = vec![api_dir.join("chunkwake-api.jar")];
    // Классы `net.minecraft.*`, распакованные из вложенного jar игры. Стоят
    // раньше самого jar: внешний их не отдаёт (они внутри `META-INF/versions`).
    let game_classes = api_dir.join("game-classes.jar");
    if game_classes.is_file() {
        jars.push(game_classes);
    }
    // Библиотеки самого jar игры (datafixerupper, brigadier, authlib): без них
    // классы `net.minecraft.*` не грузятся.
    let game_libs = api_dir.join("game-libs");
    if let Ok(entries) = std::fs::read_dir(&game_libs) {
        let mut extra: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "jar"))
            .collect();
        extra.sort();
        jars.extend(extra);
    }
    let libs = api_dir.join("libs");
    let entries = std::fs::read_dir(&libs).map_err(|e| format!("{}: {e}", libs.display()))?;
    let mut lib_jars: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jar"))
        .collect();
    lib_jars.sort();
    jars.extend(lib_jars);
    if let Some(jar) = game_jar.filter(|p| p.is_file()) {
        jars.push(jar.to_path_buf());
    }
    let joined = std::env::join_paths(jars).map_err(|e| e.to_string())?;
    Ok(joined.to_string_lossy().into_owned())
}

/// Создаёт JVM в текущем потоке и вызывает `Bridge.boot(table)`.
pub fn boot(lib: &Path, class_path: &str, table: i64) -> Result<(), String> {
    let library = load(lib)?;
    let create: libloading::Symbol<CreateJavaVm> =
        unsafe { library.get(b"JNI_CreateJavaVM\0") }.map_err(|e| e.to_string())?;

    let mut opts = vec![
        format!("-Djava.class.path={class_path}"),
        "--enable-native-access=ALL-UNNAMED".to_owned(),
        // Сигналы (Ctrl+C) остаются у сервера.
        "-Xrs".to_owned(),
    ];
    if let Ok(extra) = std::env::var("CHUNKWAKE_JVM_ARGS") {
        opts.extend(extra.split_whitespace().map(str::to_owned));
    }
    let strings: Vec<CString> = opts
        .into_iter()
        .map(|s| CString::new(s).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let mut options: Vec<JavaVMOption> = strings
        .iter()
        .map(|s| JavaVMOption {
            option_string: s.as_ptr() as *mut c_char,
            extra_info: std::ptr::null_mut(),
        })
        .collect();
    let mut args = JavaVMInitArgs {
        version: JNI_VERSION_21,
        n_options: options.len() as i32,
        options: options.as_mut_ptr(),
        ignore_unrecognized: 0,
    };
    let mut vm: *mut c_void = std::ptr::null_mut();
    let mut env: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { create(&mut vm, &mut env, &mut args as *mut _ as *mut c_void) };
    if rc != 0 {
        return Err(format!("JNI_CreateJavaVM failed: {rc}"));
    }
    // JVM живёт до конца процесса.
    std::mem::forget(library);

    let env = env as *mut *const *const c_void;
    unsafe {
        let f = |i: usize| *(*env).add(i);
        let find_class: unsafe extern "system" fn(_, *const c_char) -> *mut c_void =
            std::mem::transmute(f(FIND_CLASS));
        let get_static: unsafe extern "system" fn(
            _,
            *mut c_void,
            *const c_char,
            *const c_char,
        ) -> *mut c_void = std::mem::transmute(f(GET_STATIC_METHOD_ID));
        let call: unsafe extern "system" fn(_, *mut c_void, *mut c_void, *const i64) =
            std::mem::transmute(f(CALL_STATIC_VOID_METHOD_A));
        let check: unsafe extern "system" fn(_) -> u8 = std::mem::transmute(f(EXCEPTION_CHECK));
        let describe: unsafe extern "system" fn(_) = std::mem::transmute(f(EXCEPTION_DESCRIBE));
        let failed = |what: &str| {
            if check(env) != 0 {
                describe(env);
            }
            Err(format!("JVM: {what}"))
        };

        let class = find_class(env, c"dev/chunkwake/bridge/Bridge".as_ptr());
        if class.is_null() {
            return failed("class dev.chunkwake.bridge.Bridge not found");
        }
        let boot = get_static(env, class, c"boot".as_ptr(), c"(J)V".as_ptr());
        if boot.is_null() {
            return failed("Bridge.boot(long) not found");
        }
        // jvalue — union 8 байт; long кладётся как есть.
        call(env, class, boot, &table);
        if check(env) != 0 {
            return failed("Bridge.boot threw");
        }
    }
    Ok(())
}

#[cfg(windows)]
fn load(lib: &Path) -> Result<libloading::Library, String> {
    // Зависимости jvm.dll ищутся рядом с ней.
    const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x8;
    unsafe { libloading::os::windows::Library::load_with_flags(lib, LOAD_WITH_ALTERED_SEARCH_PATH) }
        .map(Into::into)
        .map_err(|e| format!("{}: {e}", lib.display()))
}

#[cfg(not(windows))]
fn load(lib: &Path) -> Result<libloading::Library, String> {
    unsafe { libloading::Library::new(lib) }.map_err(|e| format!("{}: {e}", lib.display()))
}

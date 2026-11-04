//! 系统语言与系统字体：把鸿蒙的 locale / 字体接进上游。
//!
//! 上游用 `sys-locale` 读系统语言，Linux 分支只看 `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`
//! 环境变量；鸿蒙应用进程不带这些变量，于是设置里的 "System" 永远解析成英文，用户只能手动
//! 去选语言。ArkTS 侧本来就能拿到真实 locale（`i18n.System.getSystemLocale()`），这里把它
//! 写回环境变量，上游自己的 "system" 逻辑（连同设置界面里的「跟随系统」）便完整生效。
//!
//! 界面语言还需要对应文字的字体：上游 `fonts::system_fallbacks()` 只扫桌面字体路径，但给了
//! 两个覆盖入口 —— `SOLIDCRAFT_FONT_DIRS`（CJK，文件名须是 `NotoSansCJK-Regular.ttc` 等）
//! 和 `CRAFT_FONTS_DIR`（阿拉伯，`NotoSansArabic-Regular.ttf`）。把鸿蒙系统字体按这两个名字
//! 挂进应用沙箱，既不 patch 上游，也不用把二十多兆字体打进 HAP。

use std::path::{Path, PathBuf};

/// 上游 `find_system_cjk_font()` 在覆盖目录里认得的文件名（同时命中 `CHINESE_FONT_KEYWORDS`
/// 的 `notosanscjk`，所以一个文件同时补上 CJK 与中文两个槽）。
const CJK_NAME: &str = "NotoSansCJK-Regular.ttc";
/// 上游 `arabic_font()` 只从 `CRAFT_FONTS_DIR` 读这个名字。
const ARABIC_NAME: &str = "NotoSansArabic-Regular.ttf";

/// 鸿蒙系统字体目录。
const SYSTEM_FONTS: &str = "/system/fonts";

/// 按界面语言挑 CJK 面。鸿蒙没有日/韩专用面，非繁体一律用 SC（含全部常用汉字）。
fn cjk_face(language: &str) -> &'static str {
    match language {
        "zh-TW" => "HarmonyOS_Sans_TC.ttf",
        _ => "HarmonyOS_Sans_SC.ttf",
    }
}

/// 系统 locale 的 BCP-47 形式（`zh_CN` → `zh-CN`）。`locale` 缺失时退回 `language`。
fn system_tag(locale: &str, language: &str) -> Option<String> {
    let tag = [locale, language].into_iter().map(str::trim).find(|tag| !tag.is_empty())?;
    Some(tag.chars().map(|c| if c == '_' { '-' } else { c }).collect())
}

/// 在 `sc_init` 里、任何 UI 与字体读取之前调用一次。
///
/// 顺序是有意义的：先写环境变量，再让 `i18n::system_language()` 首次求值（它是进程级
/// `OnceLock`，第一次读到什么就固定什么）。
pub fn install(files_dir: &str, locale: &str, language: &str) {
    let tag = system_tag(locale, language);
    if let Some(tag) = &tag {
        // sys-locale 按 LANGUAGE → LC_ALL → LC_MESSAGES → LANG 取值：先清掉可能残留的
        // 高优先级变量，再写 LANG（与桌面 Linux 的用法一致）。
        for key in ["LANGUAGE", "LC_ALL", "LC_MESSAGES"] {
            std::env::remove_var(key);
        }
        std::env::set_var("LANG", tag);
    }
    let ui_language = solidcraft_ui_egui::i18n::system_language();
    let dir = Path::new(files_dir).join("system-fonts");
    let cjk = cjk_face(ui_language);
    match stage_fonts(&dir, cjk) {
        Some(dir) => log::info!("locale: tag={tag:?} ui={ui_language:?} cjk={cjk} fonts={}", dir.display()),
        None => log::warn!("locale: tag={tag:?} ui={ui_language:?} no system CJK face {cjk}"),
    }
}

/// 把要用的系统字体挂进 `dir`，返回交给上游环境变量的目录。
fn stage_fonts(dir: &Path, cjk: &str) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    link_font(dir, CJK_NAME, cjk)?;
    // 阿拉伯面缺失只影响语言列表里的那一项显示，界面本身照常，所以不算失败。
    if link_font(dir, ARABIC_NAME, "HarmonyOS_Sans_Naskh_Arabic_UI.ttf").is_none() {
        log::info!("locale: no system Arabic face");
    }
    // 两个入口指向同一目录：SOLIDCRAFT_FONT_DIRS 的名单查 CJK，CRAFT_FONTS_DIR 查阿拉伯。
    std::env::set_var("SOLIDCRAFT_FONT_DIRS", dir);
    std::env::set_var("CRAFT_FONTS_DIR", dir);
    Some(dir.to_path_buf())
}

/// 在 `dir` 里放一份系统字体，名字由上游决定。
///
/// 鸿蒙沙箱禁止建软链（实测 EACCES），所以退回复制 —— 只在第一次（或系统换了字体文件名）时
/// 发生，之后按文件名命中直接复用。
fn link_font(dir: &Path, name: &str, face: &str) -> Option<()> {
    let source = Path::new(SYSTEM_FONTS).join(face);
    if !source.is_file() {
        return None;
    }
    let link = dir.join(name);
    // （edition 2021：let 链不可用，写成嵌套。）
    if let Ok(meta) = std::fs::symlink_metadata(&link) {
        if meta.file_type().is_symlink() && !link.exists() {
            // 断链：系统升级把字体文件改了名。
            std::fs::remove_file(&link).ok()?;
        }
    }
    if std::fs::symlink_metadata(&link).is_ok() {
        return Some(());
    }
    if std::os::unix::fs::symlink(&source, &link).is_err() {
        log::info!("locale: staging {} as {name}", source.display());
        std::fs::copy(&source, &link).ok()?;
    }
    Some(())
}

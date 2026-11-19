//! 模型积分四环总览：在岛仔原生 AI 额度页显示 WorkBuddy / TRAE / Qoder / MobileClaw
//! 四个模型的剩余积分，样式沿用岛仔内置的 iPhone 电量环（圆弧 + 两端圆点）。
//! 数据来自家庭 NAS 的 `/quotas-fast`（本地缓存取数，毫秒级），后台线程每 30 秒刷新。
//! 读取失败时保留上次数据，绝不阻塞界面。

use crate::paint::{self, Card, H};
use crate::theme::{self, W};
use eframe::egui::Color32;
use serde_json::Value;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::time::Duration;

const ENDPOINT: &str = "http://192.168.28.151:7899/quotas-fast";

#[derive(Clone, Default)]
struct One {
    pct: f32,
    remain: f64,
    total: f64,
    sub: String,
    error: String,
}

#[derive(Clone, Default)]
struct Snap {
    workbuddy: One,
    trae: One,
    qoder: One,
    mobileclaw: One,
    at: i64,
    ok: bool,
}

static CACHE: OnceLock<Mutex<Snap>> = OnceLock::new();
static FORCE: AtomicBool = AtomicBool::new(false);
static STARTED: Once = Once::new();
static SHOW_QUAD: AtomicBool = AtomicBool::new(true);

/// 是否由四环总览接管 AI 额度页（默认开启）。
pub fn show_quad() -> bool {
    SHOW_QUAD.load(Ordering::Relaxed)
}

pub fn set_quad(on: bool) {
    SHOW_QUAD.store(on, Ordering::Relaxed);
}

/// 在「原账号明细」视图顶部叠加一个返回入口，点它切回四环总览。
/// 放在标题与右上刷新之间的空白区，不碰原账号页的任何交互。
pub fn back_button(c: &Card) {
    if c.hit(150.0, 8.0, 76.0, 21.0, "quad-back").on_hover_text("返回四环总览").clicked() {
        set_quad(true);
    }
    c.text(188.0, 23.0, "‹ 四环总览", 10.5, W::Regular, theme::DIM, H::Center);
}

fn cache() -> &'static Mutex<Snap> {
    CACHE.get_or_init(|| Mutex::new(Snap::default()))
}

pub fn force_refresh() {
    FORCE.store(true, Ordering::Relaxed);
}

fn parse_one(v: Option<&Value>) -> One {
    let Some(v) = v else { return One::default() };
    One {
        pct: v.get("pct").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        remain: v.get("remain").and_then(Value::as_f64).unwrap_or(0.0),
        total: v.get("total").and_then(Value::as_f64).unwrap_or(0.0),
        sub: v.get("sub").and_then(Value::as_str).unwrap_or("").to_owned(),
        error: v.get("error").and_then(Value::as_str).unwrap_or("").to_owned(),
    }
}

fn fetch_once() {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .redirects(0)
        .build();
    let Ok(resp) = agent.get(ENDPOINT).call() else { return };
    let mut body = String::new();
    if resp.into_reader().take(1 << 16).read_to_string(&mut body).is_err() { return; }
    let Ok(v) = serde_json::from_str::<Value>(&body) else { return };
    let snap = Snap {
        workbuddy: parse_one(v.get("workbuddy")),
        trae: parse_one(v.get("trae")),
        qoder: parse_one(v.get("qoder")),
        mobileclaw: parse_one(v.get("mobileclaw")),
        at: v.get("_ts").and_then(Value::as_i64).unwrap_or(0),
        ok: true,
    };
    *cache().lock().unwrap() = snap;
}

fn fallback() -> Snap {
    // NAS 暂时不可达时的最后兜底（上次实测快照），保证四环不空白。
    let one = |pct: f32, remain: f64, total: f64, sub: &str| One {
        pct, remain, total, sub: sub.into(), error: String::new(),
    };
    Snap {
        workbuddy: one(50.0, 10796.0, 21592.0, "8 个账号"),
        trae: one(59.5, 8445.9, 14200.0, "4 个账号"),
        qoder: one(100.0, 2599.0, 2600.0, "4 个账号"),
        mobileclaw: One {
            pct: 0.0, remain: 0.0, total: 0.0,
            sub: "登录态失效，请重新登录".into(),
            error: "登录态失效".into(),
        },
        at: 0,
        ok: false,
    }
}

fn ensure_started() {
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new().name("quad-dash".into()).spawn(|| loop {
            let force = FORCE.swap(false, Ordering::Relaxed);
            fetch_once();
            std::thread::sleep(Duration::from_secs(if force { 5 } else { 180 }));
        });
    });
}

fn draw_ring(c: &Card, x: f32, y: f32, size: f32, q: &One, col: Color32, name: &str) {
    let stroke = 8.0;
    let r = size / 2.0 - stroke / 2.0 - 1.0;
    let start = -std::f32::consts::FRAC_PI_2;
    let pct = if q.error.is_empty() { q.pct.clamp(0.0, 100.0) } else { 0.0 };
    // 统一的浅灰空槽：所有环先画整圈轨道底，0% 的环也有清晰轮廓。
    // 关键：灰底、满环、部分弧三者都必须用同一个 arc 原语（折线路径），
    // 若灰底用 ring(circle_stroke 精确圆)、亮弧用 arc(折线近似)会因描边
    // 半径细微差异而错位露灰边；统一后亮弧盖住的部分就完全盖住。
    let track = Color32::from_rgb(78, 80, 88);
    let full = std::f32::consts::TAU;
    c.arc(x, y, r, 0.0, full, stroke, track);
    if pct >= 99.9 {
        c.arc(x, y, r, 0.0, full, stroke, col);
    } else if pct > 0.0 {
        let end = start + full * pct / 100.0;
        c.arc(x, y, r, start, end, stroke, col);
        for a in [start, end] {
            c.circle(x + r * a.cos(), y + r * a.sin(), stroke / 2.0, col);
        }
    }
    let bad = !q.error.is_empty();
    let center = if bad { "!".to_owned() } else { format!("{:.0}", pct) };
    c.centered_text(x, y - 4.0, &center, 24.0, W::Bold,
        if bad { Color32::from_rgb(255, 166, 77) } else { theme::WHITE }, H::Center);
    if !bad {
        c.centered_text(x, y + 14.0, "%", 10.5, W::Semibold, theme::GRAY, H::Center);
    }
    c.centered_text(x, y + size / 2.0 + 11.0, &c.fit(name, 9.5, W::Semibold, 82.0),
        9.5, W::Semibold, if bad { theme::DIM } else { col }, H::Center);
    let remain = if bad { "登录失效".to_owned() } else { format!("{:.0}", q.remain) };
    c.centered_text(x, y + size / 2.0 + 25.0, &remain, 9.5, W::Regular,
        if bad { Color32::from_rgb(255, 166, 77) } else { theme::GRAY }, H::Center);
    let tip = if q.sub.is_empty() { q.error.clone() } else { q.sub.clone() };
    c.hit(x - size / 2.0, y - size / 2.0, size, size, ("quad-ring", name))
        .on_hover_text(format!("{}\n剩余 {:.0} / 总额 {:.0}（{:.1}%）\n{}",
            name, q.remain, q.total, pct, tip));
}

pub fn page(c: &Card) {
    ensure_started();
    let snap = {
        let g = cache().lock().unwrap().clone();
        if g.ok { g } else { fallback() }
    };
    if c.hit(303.0, 8.0, 32.0, 21.0, "quad-refresh").on_hover_text("刷新积分").clicked() {
        force_refresh();
    }
    c.text(319.0, 23.0, "刷新", 10.5, W::Regular, theme::GRAY, H::Center);
    if c.hit(259.0, 12.0, 38.0, 18.0, "quad-toggle").on_hover_text("切换到原账号明细视图").clicked() {
        set_quad(false);
    }
    c.text(278.0, 23.0, "明细 ›", 9.5, W::Regular, theme::DIM, H::Center);
    let items = [
        ("WorkBuddy", &snap.workbuddy, theme::GREEN),
        ("TRAE", &snap.trae, theme::BLUE),
        ("Qoder", &snap.qoder, theme::CYAN),
        ("MobileClaw", &snap.mobileclaw, theme::YELLOW),
    ];
    // 面板宽 419、中心 x≈209.5：四环以中心对称排布，直径 72、间距 88。
    for (i, (name, q, col)) in items.into_iter().enumerate() {
        draw_ring(c, 77.5 + i as f32 * 88.0, 84.0, 72.0, q, col, name);
    }
    let status = if !snap.ok {
        "离线快照 · 正在重连 NAS".to_owned()
    } else if let Some(at) = chrono::DateTime::from_timestamp(snap.at, 0) {
        format!("NAS 实时 · {} 更新", at.with_timezone(&chrono::Local).format("%H:%M"))
    } else {
        "NAS 实时".to_owned()
    };
    c.centered_text(209.5, 160.0, &status, 8.5, W::Regular, theme::DIM, H::Center);
}

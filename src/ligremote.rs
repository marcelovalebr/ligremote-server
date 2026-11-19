// LIG Remote (ADR 0050 F2): limite real de usuários e dispositivos por licença.
//
// Ligado só quando LIGREMOTE_CHECK_URL e LIGREMOTE_LIC estão no ambiente do hbbs.
// Sem essas variáveis o hbbs se comporta igual ao oficial.
//
// - Conexão de saída (PunchHoleRequest/RequestRelay) exige o token de um usuário
//   ativo da mesma licença: GET {url}/internal/check?lic=&token=
// - Máquina nova (primeiro RegisterPk) só entra se houver vaga no plano:
//   GET {url}/internal/device?lic=&id=  -> sem vaga = NOT_DEPLOYED
//
// Respostas ficam em cache (60 s quando libera). Se o API server não responder,
// libera (fail-open): o limite é comercial, a segurança da sessão continua sendo
// a chave da instância e a senha da máquina. O alerta de API fora é do Zabbix.

use hbb_common::log;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

pub const MSG_LOGIN: &str =
    "Para conectar, entre com sua conta LIG Remote em Configurações > Conta > Login. Se ainda não tem acesso, peça ao administrador da conta da sua empresa.";

const TTL_OK: Duration = Duration::from_secs(60);
const TTL_RECUSA: Duration = Duration::from_secs(20);
const TIMEOUT: Duration = Duration::from_secs(3);
const CACHE_MAX: usize = 20_000;

struct Cfg {
    url: String,
    lic: String,
}

lazy_static::lazy_static! {
    static ref CFG: Option<Cfg> = {
        let url = std::env::var("LIGREMOTE_CHECK_URL").unwrap_or_default().trim_end_matches('/').to_owned();
        let lic = std::env::var("LIGREMOTE_LIC").unwrap_or_default();
        if url.is_empty() || lic.is_empty() || !lic.chars().all(|c| c.is_ascii_digit()) {
            None
        } else {
            log::info!("LIG Remote: limite por licença ligado (licença {}, {})", lic, url);
            Some(Cfg { url, lic })
        }
    };
    static ref TOKENS: Mutex<HashMap<String, (Instant, bool)>> = Default::default();
    static ref DEVICES: Mutex<HashMap<String, (Instant, bool)>> = Default::default();
    static ref HTTP: reqwest::Client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .no_proxy()
        .build()
        .unwrap_or_default();
    static ref ULTIMO_AVISO: Mutex<Option<Instant>> = Default::default();
}

#[inline]
pub fn enabled() -> bool {
    CFG.is_some()
}

fn cache_get(m: &Mutex<HashMap<String, (Instant, bool)>>, k: &str) -> Option<bool> {
    let g = m.lock().ok()?;
    let (t, v) = g.get(k)?;
    let ttl = if *v { TTL_OK } else { TTL_RECUSA };
    if t.elapsed() < ttl {
        Some(*v)
    } else {
        None
    }
}

fn cache_put(m: &Mutex<HashMap<String, (Instant, bool)>>, k: &str, v: bool) {
    if let Ok(mut g) = m.lock() {
        if g.len() >= CACHE_MAX {
            g.retain(|_, (t, _)| t.elapsed() < TTL_OK);
            if g.len() >= CACHE_MAX {
                g.clear();
            }
        }
        g.insert(k.to_owned(), (Instant::now(), v));
    }
}

fn aviso_api_fora(e: &str) {
    if let Ok(mut g) = ULTIMO_AVISO.lock() {
        if g.map(|t| t.elapsed() > Duration::from_secs(60)).unwrap_or(true) {
            log::warn!("LIG Remote: API server sem resposta, liberando (fail-open): {}", e);
            *g = Some(Instant::now());
        }
    }
}

async fn consulta(path: &str, params: &[(&str, &str)]) -> bool {
    let Some(cfg) = CFG.as_ref() else { return true };
    let mut q: Vec<(&str, &str)> = vec![("lic", cfg.lic.as_str())];
    q.extend_from_slice(params);
    let res = HTTP.get(format!("{}{}", cfg.url, path)).query(&q).send().await;
    match res {
        Ok(r) if r.status().is_success() => match r.json::<serde_json::Value>().await {
            Ok(v) => v.get("ok").and_then(|x| x.as_bool()).unwrap_or(true),
            Err(e) => {
                aviso_api_fora(&e.to_string());
                true
            }
        },
        Ok(r) => {
            aviso_api_fora(&format!("HTTP {}", r.status()));
            true
        }
        Err(e) => {
            aviso_api_fora(&e.to_string());
            true
        }
    }
}

/// O token (access_token do usuário logado no app) vale para esta licença?
pub async fn token_allowed(token: &str) -> bool {
    if !enabled() {
        return true;
    }
    if token.is_empty() {
        return false;
    }
    if let Some(v) = cache_get(&TOKENS, token) {
        return v;
    }
    let v = consulta("/internal/check", &[("token", token)]).await;
    cache_put(&TOKENS, token, v);
    v
}

/// A máquina nova cabe no plano? Máquina já registrada sempre passa.
pub async fn device_allowed(id: &str) -> bool {
    if !enabled() {
        return true;
    }
    if let Some(v) = cache_get(&DEVICES, id) {
        return v;
    }
    let v = consulta("/internal/device", &[("id", id)]).await;
    cache_put(&DEVICES, id, v);
    v
}

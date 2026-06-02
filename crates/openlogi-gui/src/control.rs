//! Localhost control channel for out-of-process commands — today just
//! `OpenLogi.exe --set-dpi <N>`, so a `.cmd` (game launcher, profile switch)
//! can change the mouse DPI.
//!
//! The running instance already holds the HID++ device open for its capture
//! session, so a second process opening the device itself would contend for
//! it (interleaved request/response on the same hardware). Instead, the
//! `--set-dpi` invocation connects to this loopback server and the **running**
//! instance performs the write on its open channel, then mirrors the new value
//! into the slider label. When no instance is running, the same invocation
//! falls through to opening the device directly ([`set_dpi_standalone`]).
//!
//! The listener binds `127.0.0.1` only: no firewall prompt, no network
//! exposure. The protocol is one request line in, one response line out:
//!   - `set-dpi <N>`  →  `ok <N>`  |  `err <message>`

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use openlogi_hid::{CaptureChannel, DeviceRoute};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::hardware;
use crate::state::DpiCycleState;

/// Loopback port the control server listens on. Fixed (the client needs to find
/// it without coordination) and uncommon enough to avoid clashes.
const PORT: u16 = 47800;

/// DPI bounds accepted from the control channel — the GUI slider's window.
const DPI_MIN: u16 = 200;
const DPI_MAX: u16 = 6400;

/// The loopback socket address both ends use.
fn addr() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, PORT))
}

/// Start the control server on a dedicated thread.
///
/// - `capture` lets writes reuse the capture session's open HID++ channel.
/// - `dpi_cycle` carries the active device's route (kept current as the
///   carousel selection changes), so the server knows which device to write.
/// - `label_tx` posts the applied DPI back to the GPUI loop so the slider label
///   tracks an out-of-process change.
///
/// A failed bind (port already taken) is logged, not fatal: the GUI runs fine
/// without the control channel; only `--set-dpi` against this instance is lost.
pub fn serve(
    capture: CaptureChannel,
    dpi_cycle: Arc<RwLock<DpiCycleState>>,
    label_tx: UnboundedSender<u32>,
) {
    let spawned = std::thread::Builder::new()
        .name("openlogi-control".into())
        .spawn(move || {
            let listener = match TcpListener::bind(addr()) {
                Ok(l) => l,
                Err(e) => {
                    warn!(
                        error = %e,
                        port = PORT,
                        "control server bind failed — `--set-dpi` against this instance unavailable"
                    );
                    return;
                }
            };
            info!(port = PORT, "control server listening");
            for stream in listener.incoming() {
                match stream {
                    Ok(s) => handle_conn(s, &capture, &dpi_cycle, &label_tx),
                    Err(e) => debug!(error = %e, "control accept failed"),
                }
            }
        });
    if let Err(e) = spawned {
        warn!(error = %e, "could not spawn control server thread");
    }
}

/// Read one request line, dispatch it, and write the response line back.
fn handle_conn(
    stream: TcpStream,
    capture: &CaptureChannel,
    dpi_cycle: &Arc<RwLock<DpiCycleState>>,
    label_tx: &UnboundedSender<u32>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let response = dispatch(line.trim(), capture, dpi_cycle, label_tx);
    let mut write_half = stream;
    let _ = writeln!(write_half, "{response}");
}

/// Parse and execute one command line, returning the response line.
fn dispatch(
    line: &str,
    capture: &CaptureChannel,
    dpi_cycle: &Arc<RwLock<DpiCycleState>>,
    label_tx: &UnboundedSender<u32>,
) -> String {
    let mut parts = line.split_whitespace();
    match parts.next() {
        Some("set-dpi") => match parts.next().map(str::parse::<u16>) {
            Some(Ok(dpi)) => apply_set_dpi(dpi, capture, dpi_cycle, label_tx),
            Some(Err(_)) => "err invalid DPI value".to_string(),
            None => "err missing DPI value".to_string(),
        },
        Some(other) => format!("err unknown command '{other}'"),
        None => "err empty command".to_string(),
    }
}

/// Apply a DPI write requested over the control channel, reusing the open
/// capture channel and notifying the UI of the new value.
fn apply_set_dpi(
    dpi: u16,
    capture: &CaptureChannel,
    dpi_cycle: &Arc<RwLock<DpiCycleState>>,
    label_tx: &UnboundedSender<u32>,
) -> String {
    if !(DPI_MIN..=DPI_MAX).contains(&dpi) {
        return format!("err DPI {dpi} out of range {DPI_MIN}-{DPI_MAX}");
    }
    let target = dpi_cycle.read().ok().and_then(|c| c.target.clone());
    let Some(target) = target else {
        return "err no active device".to_string();
    };
    match hardware::set_dpi_sync(Some(capture), &target, dpi) {
        Ok(reused) => {
            info!(dpi, reused, "DPI set via control channel");
            // Keep the slider label in step with the out-of-process change.
            let _ = label_tx.send(u32::from(dpi));
            format!("ok {dpi}")
        }
        Err(e) => format!("err {e}"),
    }
}

/// Back the `--set-dpi <N>` CLI flag. Tries the running instance over the
/// loopback control channel first; if nothing is listening, opens the device
/// directly. Returns a process exit code (0 success, non-zero failure).
#[must_use]
pub fn run_set_dpi_cli(raw: &str) -> i32 {
    let Ok(dpi) = raw.trim().parse::<u16>() else {
        eprintln!("--set-dpi needs a whole number, got '{raw}'");
        return 2;
    };
    match TcpStream::connect_timeout(&addr(), Duration::from_millis(400)) {
        Ok(stream) => set_dpi_via_server(stream, dpi),
        Err(_) => set_dpi_standalone(dpi),
    }
}

/// Send `set-dpi` to a connected running instance and report its response.
fn set_dpi_via_server(stream: TcpStream, dpi: u16) -> i32 {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut write_half = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("control connection failed: {e}");
            return 1;
        }
    };
    if let Err(e) = writeln!(write_half, "set-dpi {dpi}") {
        eprintln!("control write failed: {e}");
        return 1;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if let Err(e) = reader.read_line(&mut line) {
        eprintln!("control read failed: {e}");
        return 1;
    }
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("ok ") {
        println!("DPI set to {rest} (via running OpenLogi).");
        0
    } else {
        let msg = line.strip_prefix("err ").unwrap_or(line);
        eprintln!("DPI change failed: {msg}");
        1
    }
}

/// No instance running: open the first online device directly and set its DPI.
fn set_dpi_standalone(dpi: u16) -> i32 {
    if !(DPI_MIN..=DPI_MAX).contains(&dpi) {
        eprintln!("DPI {dpi} out of range {DPI_MIN}-{DPI_MAX}");
        return 1;
    }
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("runtime init failed: {e}");
            return 1;
        }
    };
    let result = rt.block_on(async {
        let route = first_online_device().await?;
        openlogi_hid::set_dpi(&route, dpi)
            .await
            .map_err(|e| format!("{e}"))
    });
    match result {
        Ok(()) => {
            println!("DPI set to {dpi}.");
            0
        }
        Err(e) => {
            eprintln!("DPI change failed: {e}");
            1
        }
    }
}

/// Build a [`DeviceRoute`] to the first online paired device — the same
/// selection rule the GUI uses for its initial target. Mirrors the CLI's
/// `first_online_device`, kept local so the GUI needn't depend on the CLI crate.
async fn first_online_device() -> Result<DeviceRoute, String> {
    let inventories = openlogi_hid::enumerate().await.map_err(|e| format!("{e}"))?;
    inventories
        .into_iter()
        .find_map(|inv| {
            let paired = inv.paired.into_iter().find(|p| p.online)?;
            Some(match inv.receiver.unique_id {
                Some(receiver_uid) => DeviceRoute::Bolt {
                    receiver_uid,
                    slot: paired.slot,
                },
                None => DeviceRoute::Direct {
                    vendor_id: inv.receiver.vendor_id,
                    product_id: inv.receiver.product_id,
                },
            })
        })
        .ok_or_else(|| "no online HID++ device found — is a Logi mouse paired?".to_string())
}

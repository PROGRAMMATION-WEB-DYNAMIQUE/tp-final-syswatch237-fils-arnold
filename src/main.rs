use chrono::Local;
use std::fmt;
use sysinfo::{System, Process};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::fs::OpenOptions;

// Structures pour les donnees
#[derive(Debug, Clone)]
struct CpuInfo {
    usage_percent: f32,
    core_count: usize,
}

#[derive(Debug, Clone)]
struct MemInfo {
    total_mb: u64,
    used_mb: u64,
    free_mb: u64,
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cpu_usage: f32,
    memory_mb: u64,
}

#[derive(Debug, Clone)]
struct SystemSnapshot {
    timestamp: String,
    cpu: CpuInfo,
    memory: MemInfo,
    top_processes: Vec<ProcessInfo>,
}

// Affichage (Etape 1)
impl fmt::Display for CpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPU: {:.1}% ({} coeurs)", self.usage_percent, self.core_count)
    }
}

impl fmt::Display for MemInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MEM: {}MB utilisés / {}MB total", self.used_mb, self.total_mb)
    }
}

impl fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "  [{:>6}] {:<20} CPU:{:>5.1}%", self.pid, self.name, self.cpu_usage)
    }
}

impl fmt::Display for SystemSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "--- SysWatch @ {} ---", self.timestamp)?;
        writeln!(f, "{}", self.cpu)?;
        writeln!(f, "{}", self.memory)?;
        writeln!(f, "Processus (Top 5):")?;
        for p in &self.top_processes {
            writeln!(f, "{}", p)?;
        }
        Ok(())
    }
}

// Gestion d'erreurs (Etape 2)
#[derive(Debug)]
enum SysWatchError {
    ErreurCollecte(String),
}

impl fmt::Display for SysWatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SysWatchError::ErreurCollecte(s) => write!(f, "Bug collecte: {}", s),
        }
    }
}
impl std::error::Error for SysWatchError {}

// Collecte réelle
fn collect_snapshot() -> Result<SystemSnapshot, SysWatchError> {
    let mut sys = System::new_all();
    sys.refresh_all();

    // Attendre un peu pour avoir des vraies valeurs CPU
    thread::sleep(Duration::from_millis(500));
    sys.refresh_all();

    let cpu_usage = sys.global_cpu_info().cpu_usage();
    let cores = sys.cpus().len();

    if cores == 0 {
        return Err(SysWatchError::ErreurCollecte("Pas de CPU !".to_string()));
    }

    let t_mem = sys.total_memory() / 1024 / 1024;
    let u_mem = sys.used_memory() / 1024 / 1024;
    let f_mem = sys.free_memory() / 1024 / 1024;

    // Trier les processus par CPU
    let mut procs = vec![];
    for p in sys.processes().values() {
        procs.push(ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string(),
            cpu_usage: p.cpu_usage(),
            memory_mb: p.memory() / 1024 / 1024,
        });
    }
    
    procs.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
    procs.truncate(5);

    Ok(SystemSnapshot {
        timestamp: Local::now().format("%H:%M:%S").to_string(),
        cpu: CpuInfo { usage_percent: cpu_usage, core_count: cores },
        memory: MemInfo { total_mb: t_mem, used_mb: u_mem, free_mb: f_mem },
        top_processes: procs,
    })
}

// Commandes speciales pour l'admin
fn run_sys_cmd(c: &str, a: &str) -> String {
    match c {
        "msg" => {
            println!("\n[ADMIN MSG] : {}", a);
            format!("OK: Message recu")
        }
        "install" => {
            println!("Installation de {}...", a);
            thread::sleep(Duration::from_secs(1));
            format!("OK: {} installe", a)
        }
        "shutdown" => {
            println!("Arret machine...");
            #[cfg(windows)]
            let _ = std::process::Command::new("shutdown").args(&["/s", "/t", "60"]).spawn();
            "Extinction dans 1 min".to_string()
        }
        "abort" => {
            #[cfg(windows)]
            let _ = std::process::Command::new("shutdown").args(&["/a"]).spawn();
            "Annulé".to_string()
        }
        _ => "Cmd inconnue".to_string()
    }
}

// Réponses reseau (Etape 3)
fn format_response(snap: &SystemSnapshot, cmd: &str) -> String {
    let c = cmd.trim().to_lowercase();
    match c.as_str() {
        "cpu" => {
            let bar = (0..10).map(|i| if i < (snap.cpu.usage_percent / 10.0) as usize { "X" } else { "." }).collect::<String>();
            format!("CPU: {:.1}% [{}]\n", snap.cpu.usage_percent, bar)
        }
        "mem" => {
            let p = (snap.memory.used_mb as f32 / snap.memory.total_mb as f32) * 100.0;
            format!("RAM: {}MB/{}MB ({:.1}%)\n", snap.memory.used_mb, snap.memory.total_mb, p)
        }
        "ps" => {
            let mut out = String::from("Top 5 Processus:\n");
            for p in &snap.top_processes {
                out.push_str(&format!("{}\n", p));
            }
            out
        }
        "all" => format!("{}\n", snap),
        "help" => "Commandes: cpu, mem, ps, all, msg <txt>, install <pkg>, shutdown, abort, quit\n".to_string(),
        "quit" => "BYE\n".to_string(),
        
        // Commandes Master
        _ if c.starts_with("msg ") => run_sys_cmd("msg", &c[4..]),
        _ if c.starts_with("install ") => run_sys_cmd("install", &c[8..]),
        "shutdown" => run_sys_cmd("shutdown", ""),
        "abort" => run_sys_cmd("abort", ""),

        _ => "Tape 'help' pour voir les commandes.\n".to_string(),
    }
}

// Logs (Etape 5 - Bonus)
fn log_to_file(msg: &str) {
    let line = format!("[{}] {}\n", Local::now().format("%Y-%m-%d %H:%M"), msg);
    print!("{}", line);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("syswatch.log") {
        let _ = f.write_all(line.as_bytes());
    }
}

// Client handler (Etape 4)
fn handle_client(mut stream: TcpStream, snap_mu: Arc<Mutex<SystemSnapshot>>) {
    let addr = stream.peer_addr().map(|a| a.to_string()).unwrap_or("?".to_string());
    log_to_file(&format!("New client: {}", addr));

    // Auth Master
    let _ = stream.write_all(b"TOKEN: ");
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut auth = String::new();
    if reader.read_line(&mut auth).is_err() || auth.trim() != "ENSPD2026" {
        let _ = stream.write_all(b"FAIL\n");
        return;
    }
    let _ = stream.write_all(b"OK\n");

    let mut line = String::new();
    while reader.read_line(&mut line).is_ok() {
        let cmd = line.trim();
        if cmd.is_empty() { break; }
        log_to_file(&format!("Cmd from {}: {}", addr, cmd));

        if cmd == "quit" { 
            let _ = stream.write_all(b"Bye!\nEND\n");
            break; 
        }

        let resp = {
            let data = snap_mu.lock().unwrap();
            format_response(&data, cmd)
        };
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.write_all(b"\nEND\n");
        line.clear();
    }
    log_to_file(&format!("Client left: {}", addr));
}

fn main() {
    println!("Lancement de SysWatch...");

    let first_snap = collect_snapshot().expect("Echec premier relevé");
    let shared_data = Arc::new(Mutex::new(first_snap));

    // Thread rafraichissement (Etape 4)
    let clone_data = Arc::clone(&shared_data);
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(5));
            if let Ok(new_data) = collect_snapshot() {
                let mut guard = clone_data.lock().unwrap();
                *guard = new_data;
                println!("[auto-refresh] OK");
            }
        }
    });

    let listener = TcpListener::bind("0.0.0.0:7878").unwrap();
    println!("Serveur pret sur le port 7878");

    for stream in listener.incoming() {
        if let Ok(s) = stream {
            let mu = Arc::clone(&shared_data);
            thread::spawn(move || handle_client(s, mu));
        }
    }
}
// Interface maitre pour le TP SysWatch
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write, Read};
use std::net::TcpStream;
use std::time::Duration;

const TOKEN: &str = "ENSPD2026";
const PORT: u16 = 7878;

// Liste des PCs a surveiller
fn get_machines() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("local-test".to_string(), "127.0.0.1".to_string());
    m.insert("PC-01".to_string(), "192.168.1.101".to_string());
    m.insert("PC-02".to_string(), "192.168.1.102".to_string());
    m.insert("PC-03".to_string(), "192.168.1.103".to_string());
    m
}

struct Session {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Session {
    fn connect(ip: &str) -> Result<Self, String> {
        let addr = format!("{}:{}", ip, PORT);
        let stream = TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            Duration::from_secs(2),
        ).map_err(|e| e.to_string())?;

        let mut sess = Session {
            stream: stream.try_clone().unwrap(),
            reader: BufReader::new(stream),
        };

        // Auth
        sess.wait_for("TOKEN: ")?;
        sess.write(TOKEN)?;
        let mut resp = String::new();
        sess.reader.read_line(&mut resp).unwrap();
        if resp.trim() != "OK" {
            return Err("Mauvais token".to_string());
        }
        Ok(sess)
    }

    fn write(&mut self, cmd: &str) -> Result<(), String> {
        self.stream.write_all(format!("{}\n", cmd).as_bytes()).map_err(|e| e.to_string())
    }

    fn wait_for(&mut self, target: &str) -> Result<(), String> {
        let mut buffer = vec![];
        let mut byte = [0u8; 1];
        loop {
            self.reader.read_exact(&mut byte).map_err(|e| e.to_string())?;
            buffer.push(byte[0]);
            let s = String::from_utf8_lossy(&buffer);
            if s.contains(target) { return Ok(()); }
            if buffer.len() > 500 { return Err("Timeout prompt".to_string()); }
        }
    }

    fn read_resp(&mut self) -> String {
        let mut result = String::new();
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line).is_err() { break; }
            if line.trim() == "END" { break; }
            result.push_str(&line);
        }
        result
    }
}

fn show_menu() {
    println!("\n--- MENU MASTER ---");
    println!("scan           : voir les machines");
    println!("add <nom> <ip> : ajouter un PC");
    println!("select <nom>   : choisir une cible");
    println!("all <cmd>      : envoyer a tous");
    println!("help / quit");
    println!("Commandes agents: cpu, mem, ps, all, msg <txt>, shutdown...");
}

fn main() {
    let mut list = get_machines();
    let mut target: Option<String> = None;
    show_menu();

    loop {
        let p = match &target {
            Some(n) => format!("[{}] > ", n),
            None => "> ".to_string(),
        };
        print!("{}", p);
        std::io::stdout().flush().unwrap();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();
        if input.is_empty() { continue; }

        match input {
            "quit" => break,
            "help" => show_menu(),
            "scan" => {
                println!("Scanning...");
                for (name, ip) in &list {
                    let ok = TcpStream::connect_timeout(&format!("{}:{}", ip, PORT).parse().unwrap(), Duration::from_millis(500)).is_ok();
                    println!("  {} ({}) -> {}", name, ip, if ok { "EN LIGNE" } else { "OFFLINE" });
                }
            }
            _ if input.starts_with("add ") => {
                let parts: Vec<&str> = input.split_whitespace().collect();
                if parts.len() == 3 {
                    list.insert(parts[1].to_string(), parts[2].to_string());
                    println!("PC ajouté.");
                }
            }
            _ if input.starts_with("select ") => {
                let name = &input[7..];
                if list.contains_key(name) {
                    target = Some(name.to_string());
                } else {
                    println!("Inconnu.");
                }
            }
            _ if input.starts_with("all ") => {
                let cmd = &input[4..];
                for (name, ip) in &list {
                    if let Ok(mut s) = Session::connect(ip) {
                        println!("-- {} --", name);
                        s.write(cmd).unwrap();
                        println!("{}", s.read_resp());
                    }
                }
            }
            cmd => {
                if let Some(name) = &target {
                    let ip = &list[name];
                    match Session::connect(ip) {
                        Ok(mut s) => {
                            s.write(cmd).unwrap();
                            println!("{}", s.read_resp());
                        }
                        Err(e) => println!("Erreur: {}", e),
                    }
                } else {
                    println!("Choisis un PC d'abord (select)");
                }
            }
        }
    }
}
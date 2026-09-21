use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn python_runtime_ping_and_infer() {
    let runtime = concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/mlx_runtime.py");
    let port = format!("{}", 18000 + (std::process::id() % 1000));
    let mut child = match Command::new("python3")
        .arg(runtime)
        .arg("--dataplane-port")
        .arg(&port)
        .arg("--bind")
        .arg("127.0.0.1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => {
            eprintln!("python3 not available; skip");
            return;
        }
    };
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).unwrap_or(0);
        if n == 0 {
            let err = {
                let mut e = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut stderr, &mut e);
                }
                e
            };
            panic!("runtime died before READY: {err}");
        }
        if line.contains("EF_ML_READY") {
            break;
        }
    }
    writeln!(stdin, r#"{{"op":"ping"}}"#).unwrap();
    stdin.flush().unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("ok"), "ping: {line}");

    writeln!(stdin, r#"{{"op":"infer","prompt":"hi","max_tokens":4}}"#).unwrap();
    stdin.flush().unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("text"), "infer: {line}");

    let assign = format!(
        r#"{{"op":"assign","assignment":{{"rank":0,"members":[{{"rank":0,"dataplane_host":"127.0.0.1","dataplane_port":{port}}}],"job":{{"kind":"train_lora","model_id":"toy-pp","parallelism":{{"n_layers":4,"hidden_size":32,"steps":2,"local_steps":1,"microbatch":1,"context_length":32}}}}}}}}"#
    );
    writeln!(stdin, "{assign}").unwrap();
    stdin.flush().unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("ok") || line.contains("rank"), "assign: {line}");

    let _ = child.kill();
}

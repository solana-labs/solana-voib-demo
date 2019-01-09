use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use stream_video::stream_video::*;

#[cfg(target_family = "unix")]
#[test]
fn test_wait_for_listener_video() -> Result<(), Box<dyn std::error::Error>> {
    let video_string_found = Arc::new(AtomicBool::new(false));

    let mut mock_video_found = Command::new("/bin/bash")
        .args(&["tests/mock_listener.sh"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdout = mock_video_found.stdout.take().unwrap();

    let video_string_found_clone = video_string_found.clone();
    thread::spawn(move || {
        wait_for_listener_video(&mut stdout).unwrap();
        video_string_found_clone.store(true, Ordering::SeqCst);
    });

    thread::sleep(Duration::from_millis(1));

    assert_eq!(video_string_found.load(Ordering::SeqCst), false);

    mock_video_found.stdin.unwrap().write_all(b"\n")?;

    for _ in 0..5000 {
        if video_string_found.load(Ordering::SeqCst) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(1));
    }

    panic!("Didn't find video start");
}

#[cfg(target_family = "unix")]
#[test]
fn test_wait_for_connecter_video() -> Result<(), Box<dyn std::error::Error>> {
    let video_string_found = Arc::new(AtomicBool::new(false));

    let mut mock_video_found = Command::new("/bin/bash")
        .args(&["tests/mock_connecter.sh"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stderr = mock_video_found.stderr.take().unwrap();

    let video_string_found_clone = video_string_found.clone();
    thread::spawn(move || {
        wait_for_connecter_video(&mut stderr).unwrap();
        video_string_found_clone.store(true, Ordering::SeqCst);
    });

    thread::sleep(Duration::from_millis(1));

    assert_eq!(video_string_found.load(Ordering::SeqCst), false);

    mock_video_found.stdin.unwrap().write_all(b"\n")?;

    for _ in 0..5000 {
        if video_string_found.load(Ordering::SeqCst) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(1));
    }

    panic!("Didn't find video start");
}

#[cfg(target_family = "unix")]
#[test]
fn test_start_video() -> Result<(), Box<dyn std::error::Error>> {
    let mut mock_video_found = Command::new("/bin/bash")
        .args(&["tests/mock_starter.sh"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdout = mock_video_found.stdout.take().unwrap();
    let stdin = mock_video_found.stdin.take().unwrap();
    let stdin = Arc::new(Mutex::new(stdin));

    let (send, recv) = channel();

    thread::spawn(move || {
        let mut buf = [0u8; 8];
        loop {
            if let Ok(len) = stdout.read(&mut buf) {
                if len != 0 {
                    send.send(String::from_utf8_lossy(&buf[..len]).into_owned())
                        .unwrap();
                }
            } else {
                break;
            }
        }
    });

    thread::sleep(Duration::from_millis(1));

    assert_eq!(recv.try_recv(), Err(TryRecvError::Empty));

    start_video(stdin)?;

    for _ in 0..5000 {
        match recv.try_recv() {
            Ok(s) => {
                assert_eq!(s, "\n");
                return Ok(());
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => panic!("Disconnect before newline"),
        }
        thread::sleep(Duration::from_millis(1));
    }

    panic!("Found no newline");
}

use log::{debug, error, info};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Copy, Clone)]
pub enum VideoManagerType {
    Listener,
    Connecter,
}

#[derive(Copy, Clone)]
pub enum VideoStatus {
    Starting,
    Stopping,
}

pub struct VideoManager {
    camera: Child,
    socket: Child,
    display: Child,
    pub manager_type: VideoManagerType,
    pub video_running: Arc<AtomicBool>,
    status_sender: Arc<Mutex<Option<glib::Sender<VideoStatus>>>>,
    // Required to keep the Arcs from being dropped before the VideoManager is dropped
    _stdin: Arc<Mutex<ChildStdin>>,
    _stdout: Option<Arc<Mutex<ChildStdout>>>,
    _stderr: Option<Arc<Mutex<ChildStderr>>>,
}

impl VideoManager {
    pub fn new_video_listener(
        port: u16,
        status_sender: Option<glib::Sender<VideoStatus>>,
    ) -> std::io::Result<VideoManager> {
        info!("Listening for connection on port {}", port);

        let (reader1, writer1) = os_pipe::pipe()?;
        let (reader2, writer2) = os_pipe::pipe()?;

        let mut camera = Command::new("raspivid")
            .args(&[
                "-k", "-v", "-n", "-fl", "-ih", "-i", "pause", "-t", "0", "-w", "800", "-h", "480",
                "-fps", "30", "-o", "-",
            ])
            .stdin(Stdio::piped())
            .stdout(writer1)
            .spawn()?;

        let socket = Command::new("nc")
            .args(&["-O", "8", "-v", "-l", &format!("{}", port)])
            .stdin(reader1)
            .stdout(writer2)
            .spawn()?;

        let mut display = Command::new("mpv")
            .args(&[
                "--profile=low-latency",
                "--fps=30",
                "--input-terminal=yes",
                "--terminal=yes",
                "-",
            ])
            .stdin(reader2)
            .stdout(Stdio::piped())
            .spawn()?;

        let stdin = camera.stdin.take().expect("Error getting camera stdin");
        let stdout = display.stdout.take().expect("Error getting display stdout");

        let stdout = Arc::new(Mutex::new(stdout));
        let stdout_clone = stdout.clone();

        let stdin = Arc::new(Mutex::new(stdin));
        let stdin_clone = stdin.clone();

        let video_running = Arc::new(AtomicBool::new(false));
        let video_running_clone = video_running.clone();

        let status_sender = Arc::new(Mutex::new(status_sender));
        let status_sender_clone = status_sender.clone();

        let _video_starter = thread::spawn(move || {
            let mut stdout = stdout_clone.lock().unwrap();
            wait_for_listener_video(&mut stdout).unwrap();
            info!("Received video, starting output stream");
            start_video(stdin_clone).unwrap();
            video_running_clone.store(true, Ordering::Release);
            let status_sender = status_sender_clone.lock().unwrap();
            if let Some(ref sender) = *status_sender {
                sender.send(VideoStatus::Starting).unwrap();
            } else {
                info!("No video status sender for video start");
            }
        });

        Ok(VideoManager {
            camera,
            socket,
            display,
            manager_type: VideoManagerType::Listener,
            video_running,
            status_sender,
            _stdin: stdin,
            _stdout: Some(stdout),
            _stderr: None,
        })
    }

    pub fn new_video_connecter(
        addr: &SocketAddr,
        status_sender: Option<glib::Sender<VideoStatus>>,
    ) -> std::io::Result<VideoManager> {
        info!("Connecting to {}", addr);

        let (reader1, writer1) = os_pipe::pipe()?;
        let (reader2, writer2) = os_pipe::pipe()?;

        let mut camera = Command::new("raspivid")
            .args(&[
                "-k", "-v", "-n", "-fl", "-ih", "-i", "pause", "-t", "0", "-w", "800", "-h", "480",
                "-fps", "30", "-o", "-",
            ])
            .stdin(Stdio::piped())
            .stdout(writer1)
            .spawn()?;

        let mut socket = Command::new("nc")
            .args(&[
                "-O",
                "8",
                "-v",
                &format!("{}", addr.ip()),
                &format!("{}", addr.port()),
            ])
            .stdin(reader1)
            .stdout(writer2)
            .stderr(Stdio::piped())
            .spawn()?;

        let display = Command::new("mpv")
            .args(&[
                "--profile=low-latency",
                "--fps=30",
                "--input-terminal=yes",
                "--terminal=yes",
                "-",
            ])
            .stdin(reader2)
            .spawn()?;

        let stdin = camera.stdin.take().expect("Error getting camera stdin");
        let stderr = socket.stderr.take().expect("Error getting socket stderr");

        let stderr = Arc::new(Mutex::new(stderr));
        let stderr_clone = stderr.clone();

        let stdin = Arc::new(Mutex::new(stdin));
        let stdin_clone = stdin.clone();

        let video_running = Arc::new(AtomicBool::new(false));
        let video_running_clone = video_running.clone();

        let status_sender = Arc::new(Mutex::new(status_sender));
        let status_sender_clone = status_sender.clone();

        let _video_starter = thread::spawn(move || {
            let mut stderr = stderr_clone.lock().unwrap();
            wait_for_connecter_video(&mut stderr).unwrap();
            info!("Established connection, starting video");
            start_video(stdin_clone).unwrap();
            video_running_clone.store(true, Ordering::Release);
            let status_sender = status_sender_clone.lock().unwrap();
            if let Some(ref sender) = *status_sender {
                sender.send(VideoStatus::Starting).unwrap();
            } else {
                info!("No video status sender for video start");
            }
        });

        Ok(VideoManager {
            camera,
            socket,
            display,
            manager_type: VideoManagerType::Connecter,
            video_running,
            status_sender,
            _stdin: stdin,
            _stdout: None,
            _stderr: Some(stderr),
        })
    }

    pub fn kill(&mut self) -> std::io::Result<()> {
        let status_sender = self.status_sender.lock().unwrap();
        if let Some(ref sender) = *status_sender {
            sender.send(VideoStatus::Stopping).unwrap();
        } else {
            info!("No video status sender for video stop");
        }
        drop(status_sender);

        self.camera
            .kill()
            .map_err(|e| error!("Failed to kill camera: {:?}", e))
            .ok();
        self.socket
            .kill()
            .map_err(|e| error!("Failed to kill socket: {:?}", e))
            .ok();
        self.display
            .kill()
            .map_err(|e| error!("Failed to kill display: {:?}", e))
            .ok();
        self.wait()?;
        Ok(())
    }

    pub fn wait(&mut self) -> std::io::Result<()> {
        while match self.camera.try_wait()? {
            None => {
                thread::sleep(Duration::from_millis(2));
                true
            }
            Some(s) => {
                info!("Camrera return: {:?}", s);
                false
            }
        } {}
        while match self.socket.try_wait()? {
            None => {
                thread::sleep(Duration::from_millis(2));
                true
            }
            Some(s) => {
                info!("Socket return: {:?}", s);
                false
            }
        } {}
        while match self.display.try_wait()? {
            None => {
                thread::sleep(Duration::from_millis(2));
                true
            }
            Some(s) => {
                info!("Display return: {:?}", s);
                false
            }
        } {}
        self.video_running.store(false, Ordering::Release);
        Ok(())
    }

    pub fn check_video_running(&mut self) -> std::io::Result<bool> {
        match self.camera.try_wait()? {
            None => {}
            Some(s) => {
                info!("Camrera return: {:?}", s);
                self.kill()?;
                return Ok(false);
            }
        }
        match self.socket.try_wait()? {
            None => {}
            Some(s) => {
                info!("Socket return: {:?}", s);
                self.kill()?;
                return Ok(false);
            }
        }
        match self.display.try_wait()? {
            None => {}
            Some(s) => {
                info!("Display return: {:?}", s);
                self.kill()?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn add_status_sender(&self, sender: glib::Sender<VideoStatus>) {
        let mut status_sender = self.status_sender.lock().unwrap();
        *status_sender = Some(sender);
    }
}

pub fn wait_for_listener_video(stdout: &mut ChildStdout) -> std::io::Result<()> {
    // raspivid is setup to start in the paused state and needs to be started
    // once the connection is established by sending a '\n' to its stdin. Listen
    // to stdout for a message that tells us that we are receiving video, then
    // start ours.

    let mut collected_output = String::default();
    let mut buffer = [0u8; 1024];
    loop {
        let len = stdout.read(&mut buffer)?;
        if len > 0 {
            collected_output.push_str(&String::from_utf8_lossy(&buffer[0..len]));
            debug!("Collected output:\n{}\n", collected_output);
        }
        if collected_output.contains("This format is marked by FFmpeg as having no timestamps!") {
            break;
        }
    }

    Ok(())
}

pub fn wait_for_connecter_video(stderr: &mut ChildStderr) -> std::io::Result<()> {
    // raspivid is setup to start in the paused state and needs to be started
    // once the connection is established by sending a '\n' to its stdin. Listen
    // to stderr for a message that tells us that we have connected, then
    // start the video.

    let mut collected_output = String::default();
    let mut buffer = [0u8; 1024];
    loop {
        let len = stderr.read(&mut buffer)?;
        if len > 0 {
            collected_output.push_str(&String::from_utf8_lossy(&buffer[0..len]));
            debug!("Collected output:\n{}\n", collected_output);
        }
        if collected_output.contains("Connection to") {
            break;
        }
    }

    Ok(())
}

pub fn start_video(stdin: Arc<Mutex<ChildStdin>>) -> std::io::Result<()> {
    info!("Starting video");

    let mut stdin = stdin.lock().unwrap();

    stdin.write_all(b"\n")?;

    Ok(())
}

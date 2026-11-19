use super::*;
use crate::airplay::fake_receiver::{respond, scripted_timer};
use crate::airplay::rtsp::{read_request, Method, RtspConnection, RtspRequest, SenderIdentity};
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::sync::mpsc;

/// A receiver on loopback that reads one request, answers it once the test
/// sends on the returned channel, and answers a second at once. A test that
/// drops the channel gets no answer. Returns its port.
fn receiver_holding_its_first_reply() -> (u16, mpsc::Sender<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind a loopback port");
    let port = listener.local_addr().expect("read the bound port").port();
    let (release, released) = mpsc::channel::<()>();
    bae_util::test_spawn::thread(move || {
        let (mut stream, _) = listener.accept().expect("the sender connects");
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        for held in [true, false] {
            let Some(request) = read_request(&mut reader).unwrap() else { return };
            if held && released.recv().is_err() {
                return;
            }
            respond(&mut stream, "200 OK", request.cseq(), &[]);
        }
    });
    (port, release)
}

/// What an owner wait told, in order.
fn told() -> (OwnerWaitTold, impl Fn(bool) + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    (OwnerWaitTold(rx), move |deciding| tx.send(deciding).expect("the test reads the notices"))
}

struct OwnerWaitTold(mpsc::Receiver<bool>);

impl OwnerWaitTold {
    fn all_so_far(&self) -> Vec<bool> {
        self.0.try_iter().collect()
    }
}

fn connect_waiting(
    port: u16,
    timer: std::sync::Arc<dyn ReplyTimer>,
) -> (RtspConnection, OwnerWaitTold) {
    let (told, deciding) = told();
    let conn = RtspConnection::connect_awaiting_owner(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        port,
        SenderIdentity::new("bae/1", "Test Mac"),
        OwnerWait::new(timer, deciding),
    )
    .expect("the receiver accepts the connection");
    (conn, told)
}

fn options() -> RtspRequest {
    RtspRequest::new(Method::Options, "*")
}

/// A reply inside the first wait tells nobody anything.
#[test]
fn a_reply_that_comes_at_once_tells_nobody() {
    let (port, release) = receiver_holding_its_first_reply();
    let (timer, driver) = scripted_timer();
    let (mut conn, told) = connect_waiting(port, timer);
    release.send(()).unwrap();
    let asking = std::thread::scope(|scope| {
        let request = scope.spawn(|| conn.request(&options()));
        assert_eq!(driver.next_wait(), NOTICE_AFTER);
        driver.reply_arrives();
        request.join().unwrap()
    });
    assert!(asking.expect("the reply arrives").is_success());
    assert_eq!(told.all_so_far(), Vec::<bool>::new());
}

/// The reply of a receiver whose owner decides late comes inside the minute:
/// the first wait ends at five seconds with a notice, the second lasts the
/// rest of the minute, and the reply ends the notice.
#[test]
fn a_reply_held_past_the_notice_comes_inside_the_minute() {
    let (port, release) = receiver_holding_its_first_reply();
    let (timer, driver) = scripted_timer();
    let (mut conn, told) = connect_waiting(port, timer);
    let asking = std::thread::scope(|scope| {
        let request = scope.spawn(|| conn.request(&options()));
        assert_eq!(driver.next_wait(), NOTICE_AFTER);
        driver.elapse();
        assert_eq!(driver.next_wait(), OWNER_DECIDES_WITHIN - NOTICE_AFTER);
        assert_eq!(told.all_so_far(), [true]);
        release.send(()).unwrap();
        driver.reply_arrives();
        request.join().unwrap()
    });
    assert!(asking.expect("the reply arrives").is_success());
    assert_eq!(told.all_so_far(), [false]);
}

/// A reply that has not come when the minute ends fails the request as a
/// timeout.
#[test]
fn a_reply_held_for_the_whole_minute_times_out() {
    let (port, _release) = receiver_holding_its_first_reply();
    let (timer, driver) = scripted_timer();
    let (mut conn, told) = connect_waiting(port, timer);
    let asking = std::thread::scope(|scope| {
        let request = scope.spawn(|| conn.request(&options()));
        driver.next_wait();
        driver.elapse();
        driver.next_wait();
        driver.elapse();
        request.join().unwrap()
    });
    assert_eq!(asking.expect_err("no reply").kind(), io::ErrorKind::TimedOut);
    assert_eq!(told.all_so_far(), [true]);
}

/// Only the first request waits for the owner: the next one is bounded as
/// every request is.
#[test]
fn the_second_request_waits_no_longer_than_any_request() {
    let (port, release) = receiver_holding_its_first_reply();
    let (timer, driver) = scripted_timer();
    let (mut conn, _told) = connect_waiting(port, timer);
    release.send(()).unwrap();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| conn.request(&options()).map(|_| ()));
        driver.next_wait();
        driver.reply_arrives();
        first.join().unwrap().expect("the first reply arrives");
    });
    assert!(conn.request(&options()).expect("the second reply arrives").is_success());
    assert!(driver.asked_nothing_more());
}

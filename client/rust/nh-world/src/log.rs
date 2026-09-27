use std::collections::VecDeque;

/// ATR_URGENT: the message must not be missed.
pub const ATR_URGENT: i32 = 16;
/// ATR_NOHISTORY: a transient message that does not belong in the history.
pub const ATR_NOHISTORY: i32 = 32;

/// How many messages the log keeps.
pub const LOG_CAPACITY: usize = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// 1, 2, 3... in arrival order; never reused.
    pub seq: u64,
    pub text: String,
    /// Display attribute (ATR_BOLD...), without the URGENT/NOHISTORY bits.
    pub attr: i32,
    /// Game turn when it arrived, if the status line showed one.
    pub turn: Option<i64>,
    pub urgent: bool,
    /// Restored from a save (putmsghistory), not said in this session.
    pub from_history: bool,
}

/// The message history, newest last.
#[derive(Debug, Clone, Default)]
pub struct MessageLog {
    messages: VecDeque<Message>,
    last_seq: u64,
}

impl MessageLog {
    pub fn new() -> MessageLog {
        MessageLog::default()
    }

    pub fn push(&mut self, text: String, attr: i32, turn: Option<i64>, from_history: bool) {
        self.last_seq += 1;
        if self.messages.len() == LOG_CAPACITY {
            self.messages.pop_front();
        }
        self.messages.push_back(Message {
            seq: self.last_seq,
            text,
            attr: attr & !(ATR_URGENT | ATR_NOHISTORY),
            turn,
            urgent: attr & ATR_URGENT != 0,
            from_history,
        });
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Message> {
        self.messages.iter()
    }

    /// The last seq handed out: the newest message's, unless it was
    /// retracted; 0 while nothing was ever logged. It changes with every
    /// push and retraction.
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Take the newest message back (it turned out to be a prompt). Its seq
    /// is not reused: `last_seq` moves on, so views notice the change.
    pub fn retract_newest(&mut self) -> Option<Message> {
        let m = self.messages.pop_back()?;
        self.last_seq += 1;
        Some(m)
    }

    /// Messages newer than `seq`.
    pub fn since(&self, seq: u64) -> impl Iterator<Item = &Message> {
        self.messages.iter().filter(move |m| m.seq > seq)
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_keeps_the_newest_thousand_with_their_numbers() {
        let mut log = MessageLog::new();
        assert_eq!(log.last_seq(), 0);
        for i in 0..1005 {
            log.push(format!("m{i}"), 1 | ATR_URGENT, Some(i), false);
        }
        assert_eq!(log.len(), LOG_CAPACITY);
        let first = log.iter().next().unwrap();
        assert_eq!((first.seq, first.text.as_str()), (6, "m5"));
        assert_eq!(first.attr, 1);
        assert!(first.urgent);
        assert_eq!(log.iter().next_back().unwrap().seq, 1005);
        let recent: Vec<_> = log.since(1003).map(|m| m.text.as_str()).collect();
        assert_eq!(recent, vec!["m1003", "m1004"]);
        let taken = log.retract_newest().unwrap();
        assert_eq!(taken.text, "m1004");
        assert_eq!(log.last_seq(), 1006);
        log.push("next".into(), 0, None, false);
        assert_eq!(log.iter().next_back().unwrap().seq, 1007);
    }
}

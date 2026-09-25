//! Commands the fork sends to the worker, and the colours of phone labels.

use crate::model::ChatId;

/// Work the fork asks of the worker through `Command::Fork`.
#[derive(Clone, Debug)]
pub enum ForkCommand {
    /// Replays labels and archived chats from the phone again, from scratch.
    ResyncPhone,
    /// Removes chats from this computer, and from the phone where it can.
    /// Unlike a normal delete it does not wait for the phone, because the
    /// chats it is meant for are the ones the phone cannot handle.
    PurgeChats(Vec<ChatId>),
    /// Sends the chat list again, after a resync.
    Refresh,
}

/// WhatsApp Business label colours, by the index the phone syncs.
const LABEL_COLORS: [&str; 20] = [
    "#ff9485", "#64c4ff", "#ffd429", "#dfaef0", "#99b6c1", "#55ccb3", "#ff9dff", "#d3a91d",
    "#6d7cce", "#d7e752", "#00d0e2", "#ffc5c7", "#93ceac", "#f74848", "#00a0f2", "#83e422",
    "#ffaf04", "#b5ebff", "#9ba6ff", "#9368cf",
];

/// The colour of a phone label index; unknown indexes wrap around.
pub fn label_color(index: Option<i32>) -> &'static str {
    let index = index.unwrap_or(0).rem_euclid(LABEL_COLORS.len() as i32) as usize;
    LABEL_COLORS[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_colours_wrap_and_default() {
        assert_eq!(label_color(None), "#ff9485");
        assert_eq!(label_color(Some(21)), "#64c4ff");
        assert_eq!(label_color(Some(-1)), "#9368cf");
    }
}

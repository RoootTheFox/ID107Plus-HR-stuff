use enigo::{Enigo, Key, KeyboardControllable};

pub(crate) fn key_down(enigo:&mut Enigo, key:Key) {
    enigo.key_down(key);
}

pub(crate) fn key_up(enigo:&mut Enigo, key:Key) {
    enigo.key_up(key);
}
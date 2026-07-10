use crate::model::PhoneticLevel;

pub trait PhoneticProvider {
    fn phonetic_level(&self) -> PhoneticLevel;
    fn get_phonetic(&self, text: &str) -> String;
}

use std::{env};


pub fn get_port() -> u16 {
    match env::var("PORT") {
        Ok(val) =>
            match val.parse::<u16>() {
                Ok(p) => p,
                Err(_) => 8080
            }

        Err(_e) => 8080,
    }
}
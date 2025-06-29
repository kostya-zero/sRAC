use std::{
    error::Error,
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
};

use chrono::Local;
use log::info;

use crate::ctx::{Account, Context, add_message};

pub fn on_total_size(ctx: Arc<Context>, _: SocketAddr) -> Result<u64, Box<dyn Error>> {
    let messages_len = ctx.messages.read().unwrap().len() as u64;
    let offset = ctx.messages_offset.load(Ordering::SeqCst);

    if let Some(splash) = &ctx.args.splash {
        Ok(messages_len + splash.len() as u64 + offset)
    } else {
        Ok(messages_len + offset)
    }
}

pub fn on_total_data(
    ctx: Arc<Context>,
    _: SocketAddr,
    _: Option<u64>, // sent_size
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut messages = ctx.messages.read().unwrap().clone();
    let offset = ctx.messages_offset.load(Ordering::SeqCst);

    let mut messages = if offset > 0 {
        let mut buf = vec![0; offset as usize];
        buf.append(&mut messages);
        buf
    } else {
        messages
    };

    if let Some(splash) = &ctx.args.splash {
        messages.append(&mut splash.clone().as_bytes().to_vec());
    }

    Ok(messages)
}

pub fn on_chunked_data(
    ctx: Arc<Context>,
    _: SocketAddr,
    _: Option<u64>, // sent_size
    client_has: u64,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let messages = ctx.messages.read().unwrap().clone();
    let offset = ctx.messages_offset.load(Ordering::SeqCst);
    let client_has = if let Some(splash) = &ctx.args.splash {
        client_has - splash.len() as u64
    } else {
        client_has
    };

    if client_has <= offset {
        Ok(messages)
    } else {
        let client_has = (client_has - offset) as usize;
        Ok(messages[client_has..].to_vec())
    }
}

pub fn on_send_message(
    ctx: Arc<Context>,
    addr: SocketAddr,
    message: Vec<u8>,
) -> Result<(), Box<dyn Error>> {
    if !ctx.args.auth_only {
        let mut message = message;
        message.truncate(ctx.args.message_limit);

        add_message(&message.clone(), ctx.clone(), Some(addr.ip()))?;
    }
    Ok(())
}

pub fn on_send_auth_message(
    ctx: Arc<Context>,
    _: SocketAddr,
    name: &str,
    password: &str,
    text: &str,
) -> Result<Option<u8>, Box<dyn Error>> {
    if let Some(acc) = ctx.get_account(name) {
        if acc.check_password(password) {
            let mut name = name.to_string();
            name.truncate(256); // FIXME: softcode this

            let mut password = password.to_string();
            password.truncate(256); // FIXME: softcode this

            let mut text = text.to_string();
            text.truncate(ctx.args.message_limit);

            add_message(&text.as_bytes(), ctx.clone(), None)?;

            Ok(None)
        } else {
            Ok(Some(0x02))
        }
    } else {
        Ok(Some(0x01))
    }
}

pub fn on_register_user(
    ctx: Arc<Context>,
    addr: SocketAddr,
    name: &str,
    password: &str,
) -> Result<Option<u8>, Box<dyn Error>> {
    let addr = addr.ip().to_string();

    let now: i64 = Local::now().timestamp_millis();

    if ctx.get_account(name).is_some()
        || (if let Some(acc) = ctx.get_account_by_addr(&addr) {
            ((now - acc.date()) as usize) < 1000 * ctx.args.register_timeout
        } else {
            false
        })
    {
        return Ok(Some(0x01));
    }

    let account = Account::new(name.to_string(), password.to_string(), addr, now);

    info!("user registered: {name}");

    ctx.push_account(account)?;

    Ok(None)
}

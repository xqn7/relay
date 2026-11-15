//! Checks the email of a new account. Relay never sends email (accounts are local), so an
//! address can't be proven to be real; what can be checked: that it's well formed, that it's not
//! a typo of a big provider or a throwaway service, and that its domain accepts mail (an MX
//! record, asked through Windows' own DNS resolver).

use crate::error::{AppError, AppResult};
use std::time::Duration;
use windows::core::HSTRING;
use windows::Win32::Foundation::{DNS_ERROR_RCODE_NAME_ERROR, DNS_INFO_NO_RECORDS, ERROR_SUCCESS};
use windows::Win32::NetworkManagement::Dns::{DnsFree, DnsFreeRecordList, DnsQuery_W, DNS_QUERY_STANDARD, DNS_RECORDA, DNS_RECORDW, DNS_TYPE_MX};

/// Real mail domains answer within a second; a name that doesn't exist can keep the resolver
/// busy for 10 s and more (retries), so a lookup still running by now is treated as one.
const DNS_TIMEOUT: Duration = Duration::from_secs(6);

/// Misspellings of big providers, with what was meant.
const TYPOS: &[(&str, &str)] = &[
    ("gmial.com", "gmail.com"),
    ("gamil.com", "gmail.com"),
    ("gmai.com", "gmail.com"),
    ("gmaill.com", "gmail.com"),
    ("gmail.co", "gmail.com"),
    ("gmail.cm", "gmail.com"),
    ("gmail.con", "gmail.com"),
    ("gnail.com", "gmail.com"),
    ("hotmial.com", "hotmail.com"),
    ("hotmal.com", "hotmail.com"),
    ("hotmail.co", "hotmail.com"),
    ("hotmail.con", "hotmail.com"),
    ("outlok.com", "outlook.com"),
    ("outlook.co", "outlook.com"),
    ("outllook.com", "outlook.com"),
    ("yahooo.com", "yahoo.com"),
    ("yaho.com", "yahoo.com"),
    ("yahoo.co", "yahoo.com"),
    ("icloud.co", "icloud.com"),
    ("iclod.com", "icloud.com"),
    ("gmx.ed", "gmx.de"),
    ("gmx.dee", "gmx.de"),
    ("web.ed", "web.de"),
    ("t-onlien.de", "t-online.de"),
];

/// Throwaway inbox services: an account under one of these can't be told apart from no email.
const DISPOSABLE: &[&str] = &[
    "mailinator.com",
    "10minutemail.com",
    "guerrillamail.com",
    "guerrillamail.net",
    "sharklasers.com",
    "temp-mail.org",
    "tempmail.com",
    "tempmail.net",
    "yopmail.com",
    "yopmail.net",
    "trashmail.com",
    "trashmail.de",
    "getnada.com",
    "dispostable.com",
    "throwawaymail.com",
    "maildrop.cc",
    "fakeinbox.com",
    "mintemail.com",
    "mohmal.com",
    "emailondeck.com",
    "spam4.me",
    "wegwerfmail.de",
    "einrot.com",
];

/// RFC-style shape: a sensible local part, a domain of letters/digits/hyphens, a real TLD.
pub fn well_formed(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else { return false };
    let local_ok = !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local.chars().all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+/=?^_`{|}~.-".contains(c));
    let labels: Vec<&str> = domain.split('.').collect();
    let domain_ok = domain.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && labels.last().is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()));
    local_ok && domain_ok
}

/// The provider meant by a misspelled domain, if it's a known misspelling.
pub fn typo_of(domain: &str) -> Option<&'static str> {
    TYPOS.iter().find(|(typo, _)| *typo == domain).map(|(_, meant)| *meant)
}

pub fn is_disposable(domain: &str) -> bool {
    DISPOSABLE.contains(&domain)
}

/// Whether the domain has a mail server. `Err` when DNS couldn't be asked (no internet).
fn accepts_mail(domain: &str) -> AppResult<bool> {
    let name = HSTRING::from(domain);
    let mut results: *mut DNS_RECORDA = std::ptr::null_mut();
    // SAFETY: `results` receives a record list that DnsFree releases below; the name lives
    // for the whole call.
    let status = unsafe { DnsQuery_W(&name, DNS_TYPE_MX, DNS_QUERY_STANDARD, None, &mut results, None) };
    if status == DNS_ERROR_RCODE_NAME_ERROR || status.0 == DNS_INFO_NO_RECORDS as u32 {
        return Ok(false);
    }
    if status != ERROR_SUCCESS {
        return Err(AppError::new("email_check_offline", format!("DNS lookup failed: {}", status.0)));
    }
    let mut found = false;
    // SAFETY: DnsQuery_W (the wide-string variant) fills the list with DNS_RECORDW entries;
    // the list is only read while it's alive.
    unsafe {
        let mut rec = results as *const DNS_RECORDW;
        while let Some(r) = rec.as_ref() {
            if r.wType == DNS_TYPE_MX.0 {
                // A "null MX" (RFC 7505: exchange ".") says the domain takes no mail.
                let exchange = r.Data.MX.pNameExchange.to_string().unwrap_or_default();
                if !exchange.trim_matches('.').is_empty() {
                    found = true;
                    break;
                }
            }
            rec = r.pNext;
        }
        DnsFree(Some(results as *const _), DnsFreeRecordList);
    }
    Ok(found)
}

/// Every check for a new account's (already lowercased) email. `online`: whether this PC has
/// internet, to tell "no internet" from "this domain doesn't exist" when DNS gives no answer.
pub async fn check(email: &str, online: bool) -> AppResult<()> {
    if !well_formed(email) {
        return Err(AppError::new("invalid_email", ""));
    }
    let domain = email.rsplit_once('@').map(|(_, d)| d.to_string()).unwrap_or_default();
    if let Some(meant) = typo_of(&domain) {
        return Err(AppError::new("email_typo", meant));
    }
    if is_disposable(&domain) {
        return Err(AppError::new("email_disposable", ""));
    }
    if !online {
        return Err(AppError::new("email_check_offline", "no internet"));
    }
    let lookup = tokio::task::spawn_blocking(move || accepts_mail(&domain));
    match tokio::time::timeout(DNS_TIMEOUT, lookup).await {
        Ok(Ok(Ok(true))) => Ok(()),
        // No mail server, a DNS error or no answer while the internet works: no such mail domain.
        _ => Err(AppError::new("email_domain", "")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape() {
        for ok in ["anna@gmail.com", "a.b+tag@mail.example.de", "x_y@sub.domain.co.uk"] {
            assert!(well_formed(ok), "{ok}");
        }
        for bad in ["anna", "anna@", "@gmail.com", "anna@gmail", "an..na@gmail.com", ".anna@gmail.com", "anna@-x.com", "anna@x.c0m", "an na@gmail.com", "anna@gmail.c"] {
            assert!(!well_formed(bad), "{bad}");
        }
    }

    #[test]
    fn typos_and_throwaways() {
        assert_eq!(typo_of("gmial.com"), Some("gmail.com"));
        assert_eq!(typo_of("gmail.com"), None);
        assert!(is_disposable("mailinator.com"));
        assert!(!is_disposable("gmail.com"));
    }

    /// Asks the real DNS: run with `cargo test -- --ignored` (needs internet).
    #[test]
    #[ignore]
    fn live_mail_domains() {
        assert!(accepts_mail("gmail.com").unwrap());
        assert!(accepts_mail("gmx.de").unwrap());
        assert!(!accepts_mail("no-such-domain-relay-test-4471.com").unwrap());
        eprintln!("klh.com accepts mail: {:?}", accepts_mail("klh.com"));
    }
}

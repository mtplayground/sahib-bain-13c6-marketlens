#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub sub: String,
    pub email: String,
    pub email_verified: bool,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub name: Option<String>,
    pub picture_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserProfile {
    pub sub: String,
    pub email: String,
    pub email_verified: bool,
    pub name: Option<String>,
    pub picture_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpsertUser {
    pub sub: String,
    pub email: String,
    pub email_verified: bool,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub name: Option<String>,
    pub picture_url: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UserModelError {
    #[error("user subject cannot be empty")]
    EmptySubject,
    #[error("user email cannot be empty")]
    EmptyEmail,
}

impl UserProfile {
    pub fn new(
        sub: impl Into<String>,
        email: impl Into<String>,
        email_verified: bool,
        name: Option<String>,
        picture_url: Option<String>,
    ) -> Result<Self, UserModelError> {
        let sub = normalized_required(sub.into(), UserModelError::EmptySubject)?;
        let email = normalized_required(email.into(), UserModelError::EmptyEmail)?;

        Ok(Self {
            sub,
            email,
            email_verified,
            name: normalize_optional(name),
            picture_url: normalize_optional(picture_url),
        })
    }

    pub fn into_upsert(self, observed_at: DateTime<Utc>) -> UpsertUser {
        UpsertUser {
            sub: self.sub,
            email: self.email,
            email_verified: self.email_verified,
            email_verified_at: self.email_verified.then_some(observed_at),
            name: self.name,
            picture_url: self.picture_url,
        }
    }
}

fn normalized_required(value: String, error: UserModelError) -> Result<String, UserModelError> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        Err(error)
    } else {
        Ok(normalized)
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::{UserModelError, UserProfile};

    #[test]
    fn normalizes_profile_input_for_upsert() {
        let Some(observed_at) = chrono::Utc
            .with_ymd_and_hms(2026, 7, 20, 23, 4, 0)
            .single()
        else {
            panic!("test timestamp should be valid");
        };
        let profile = match UserProfile::new(
            " sub-123 ",
            " trader@example.com ",
            true,
            Some(" Trader ".to_owned()),
            Some(" ".to_owned()),
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("profile should be valid: {error}"),
        };
        let upsert = profile.into_upsert(observed_at);

        assert_eq!(upsert.sub, "sub-123");
        assert_eq!(upsert.email, "trader@example.com");
        assert_eq!(upsert.name, Some("Trader".to_owned()));
        assert_eq!(upsert.picture_url, None);
        assert_eq!(upsert.email_verified_at, Some(observed_at));
    }

    #[test]
    fn rejects_empty_subject_and_email() {
        assert_eq!(
            UserProfile::new(" ", "trader@example.com", false, None, None),
            Err(UserModelError::EmptySubject)
        );
        assert_eq!(
            UserProfile::new("sub-123", " ", false, None, None),
            Err(UserModelError::EmptyEmail)
        );
    }
}

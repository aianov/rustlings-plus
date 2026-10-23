use contracts::Username;
use service_runtime::Secret;

use super::{AuthError, AuthService, Letter, random_hex, token};
use crate::repository::{OneTime, Purpose};

impl AuthService {
    async fn mail_token(&self, purpose: Purpose, user_id: u64, to: &str) -> Result<(), AuthError> {
        let now = self.clock.now();
        let secret = random_hex(16)?;
        let (lifetime, subject, path) = match purpose {
            Purpose::ConfirmEmail => (
                self.lifetimes.confirm,
                "подтверди почту",
                "/auth/email/confirm",
            ),
            Purpose::ResetPassword => {
                (self.lifetimes.reset, "сброс пароля", "/auth/password/reset")
            }
        };
        let once = OneTime {
            purpose,
            user_id,
            expires: now + lifetime,
        };
        self.once
            .issue(token::hmac::sha256_hex(&secret), once)
            .await;
        self.mailer.send(Letter {
            to: to.to_string(),
            subject: subject.to_string(),
            link: format!("{}{path}?token={secret}", self.public_url),
        });
        Ok(())
    }

    pub async fn request_confirmation(&self, access: &str) -> Result<(), AuthError> {
        let principal = self.validate(access).await?;
        let account = self
            .accounts
            .get(principal.user_id)
            .await
            .ok_or(AuthError::TokenInvalid)?;
        let email = account.email.clone().ok_or(AuthError::NoEmail)?;
        self.mail_token(Purpose::ConfirmEmail, account.user_id, &email)
            .await
    }

    async fn redeem(&self, purpose: Purpose, secret: &str) -> Result<u64, AuthError> {
        let hash = token::hmac::sha256_hex(secret);
        let now = self.clock.now();
        self.once
            .take(purpose, hash, now)
            .await
            .ok_or(AuthError::OnceInvalid)
    }

    pub async fn confirm_email(&self, secret: &str) -> Result<(), AuthError> {
        let user_id = self.redeem(Purpose::ConfirmEmail, secret).await?;
        let mut account = self
            .accounts
            .get(user_id)
            .await
            .ok_or(AuthError::OnceInvalid)?;
        account.email_verified = true;
        Ok(self.accounts.update(account).await?)
    }

    pub async fn forgot_password(&self, name: &Username) -> Result<(), AuthError> {
        let Some(account) = self.accounts.find(name.as_ref().to_string()).await else {
            return Ok(());
        };
        let Some(email) = account.email.clone() else {
            return Ok(());
        };
        self.mail_token(Purpose::ResetPassword, account.user_id, &email)
            .await
    }

    pub async fn reset_password(
        &self,
        secret: &str,
        password: &Secret<String>,
    ) -> Result<usize, AuthError> {
        let user_id = self.redeem(Purpose::ResetPassword, secret).await?;
        let mut account = self
            .accounts
            .get(user_id)
            .await
            .ok_or(AuthError::OnceInvalid)?;
        account.password = Some(self.passwords.hash(password).await?);
        self.accounts.update(account).await?;
        let closed = self.sessions.revoke_user(user_id, None).await;
        self.announce(user_id, &closed).await;
        Ok(closed.len())
    }
}

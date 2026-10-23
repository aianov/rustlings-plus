use service_runtime::Secret;

use super::{AuthError, AuthService, Verdict};

impl AuthService {
    async fn deny_current(&self, jti: &str, expires: u64) {
        let until = expires + self.policy.leeway;
        self.sessions.deny(jti.to_string(), until).await;
    }

    pub async fn logout(&self, access: &str) -> Result<(), AuthError> {
        let principal = self.validate(access).await?;
        if let Some(row) = self.sessions.family_of(principal.jti.clone()).await {
            self.sessions.revoke_family(row.family).await;
        }
        self.deny_current(&principal.jti, principal.expires).await;
        Ok(())
    }

    pub async fn logout_all(&self, access: &str) -> Result<usize, AuthError> {
        let principal = self.validate(access).await?;
        let closed = self.sessions.revoke_user(principal.user_id, None).await;
        self.deny_current(&principal.jti, principal.expires).await;
        Ok(closed)
    }

    pub async fn change_password(
        &self,
        access: &str,
        old: &Secret<String>,
        new: &Secret<String>,
    ) -> Result<usize, AuthError> {
        let principal = self.validate(access).await?;
        let mut account = self
            .accounts
            .get(principal.user_id)
            .await
            .ok_or(AuthError::TokenInvalid)?;
        let verdict = self
            .passwords
            .verify(old, account.password.as_deref())
            .await?;
        if verdict == Verdict::Wrong {
            return Err(AuthError::InvalidCredentials);
        }
        account.password = Some(self.passwords.hash(new).await?);
        self.accounts.update(account).await?;
        let current = self.sessions.family_of(principal.jti).await;
        let keep = current.map(|row| row.family);
        Ok(self.sessions.revoke_user(principal.user_id, keep).await)
    }
}

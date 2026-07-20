#[cfg(test)]
mod tests {
    use crate::*;
    use sentinel_sandbox::SandboxPolicy;

    #[tokio::test]
    async fn test_local_executor_policy_denied() {
        let mut policy = SandboxPolicy::default();
        // Allow ONLY "cargo"
        policy.allowed_commands = vec!["cargo".into()];
        
        let executor = LocalExecutor::new(Some(policy));

        // "cargo" should be allowed
        let res = executor.exec("cargo", &["--version"], None).await;
        assert!(res.is_ok());
        let output = res.unwrap();
        assert_eq!(output.exit_code, 0);

        // "rm" should be denied
        let res2 = executor.exec("rm", &["-rf", "/"], None).await;
        assert!(res2.is_err());
        match res2.unwrap_err() {
            ExecError::PermissionDenied(msg) => {
                assert!(msg.contains("denied by sandbox policy"));
            }
            other => panic!("Expected PermissionDenied, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_local_executor_write_denied() {
        let mut policy = SandboxPolicy::default();
        // Read/write only allowed in current dir by default, let's restrict write entirely
        policy.write_paths = vec![];

        let executor = LocalExecutor::new(Some(policy));

        // Write should fail
        let res = executor.write_file("/tmp/test.txt", "hello").await;
        assert!(res.is_err());
        match res.unwrap_err() {
            ExecError::PermissionDenied(msg) => {
                assert!(msg.contains("Write to"));
                assert!(msg.contains("denied by sandbox policy"));
            }
            other => panic!("Expected PermissionDenied, got {:?}", other),
        }
    }
}

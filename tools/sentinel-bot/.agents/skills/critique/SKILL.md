# Critique Skill

Technical and logical validation layer for the Sentinel Bot.

## Capabilities
- Review proposed changes for correctness and safety
- Validate actor-awareness and anti-spam protocols
- Check for scope creep (one thing per run)
- Verify CI policy compliance

## Usage
Loaded by the Brain in Phase 2 before publishing changes.

## Criteria
- **Correctness**: Does the change solve the identified problem?
- **Safety**: No destructive operations without safeguards
- **Scope**: Single improvement per run?
- **Spam**: No duplicate or unnecessary operations
- **Policy**: Complies with `ci-policy.toml` permissions

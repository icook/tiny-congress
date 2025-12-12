# Pre-Release Checklist

Complete before deploying to production.

## CI/CD

- [ ] All CI checks passing on master
- [ ] No pending PRs that should be included
- [ ] Release branch created (if using release branches)
- [ ] Version bumped appropriately (semver)

## Testing

- [ ] Full test suite passes
- [ ] E2E tests pass against staging
- [ ] Manual smoke test on staging:
  - [ ] Core user flows work
  - [ ] No console errors
  - [ ] Performance acceptable
- [ ] Load testing completed (if significant changes)

## Database

- [ ] All migrations applied to staging
- [ ] Migration rollback tested
- [ ] No long-running migrations that could lock tables
- [ ] Backup verified recent

## Dependencies

- [ ] No known security vulnerabilities
- [ ] No deprecated dependencies with EOL
- [ ] Third-party service status checked

## Configuration

- [ ] Environment variables documented
- [ ] Secrets rotated if needed
- [ ] Feature flags configured correctly
- [ ] Rate limits appropriate

## Monitoring

- [ ] Alerts configured for new functionality
- [ ] Dashboards updated
- [ ] Log levels appropriate (not debug in prod)
- [ ] Error tracking enabled

## Documentation

- [ ] CHANGELOG updated
- [ ] API docs updated (if endpoints changed)
- [ ] User-facing docs updated
- [ ] Runbook updated for new failure modes

## Rollback plan

- [ ] Previous version tagged and deployable
- [ ] Rollback procedure documented
- [ ] Database rollback scripts ready (if migrations)
- [ ] Feature flags allow partial rollback

## Communication

- [ ] Team notified of deployment window
- [ ] On-call engineer aware
- [ ] Status page ready to update (if applicable)
- [ ] Customer communication prepared (if breaking changes)

## Post-deploy verification

- [ ] Health endpoints responding
- [ ] Key metrics stable
- [ ] Error rates normal
- [ ] No new errors in logs
- [ ] Spot-check critical user flows

## Sign-off

- [ ] Engineering lead approved
- [ ] QA signed off (if applicable)
- [ ] Product owner approved (if user-facing)

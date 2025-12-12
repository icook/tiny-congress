# Incident Checklist

Use during active incidents. Focus on resolution first, documentation second.

## Detection (first 5 minutes)

- [ ] Acknowledge alert/report
- [ ] Verify incident is real (not false positive)
- [ ] Assess severity:
  - **P1**: Service down, all users affected
  - **P2**: Major feature broken, many users affected
  - **P3**: Minor feature broken, some users affected
  - **P4**: Cosmetic issue, workaround available
- [ ] Start incident channel/thread
- [ ] Assign incident commander (if P1/P2)

## Triage (next 15 minutes)

- [ ] Identify affected systems
- [ ] Check recent deployments: `gh run list --limit 5`
- [ ] Check recent changes: `git log --oneline -10`
- [ ] Review error logs: `kubectl logs -l app=tc-api --tail=100`
- [ ] Check external dependencies status
- [ ] Communicate initial assessment to stakeholders

## Mitigation

### If caused by recent deploy
- [ ] Consider immediate rollback
- [ ] Rollback command: `kubectl rollout undo deployment/<name>`
- [ ] Verify rollback successful
- [ ] Confirm service restored

### If caused by data issue
- [ ] Identify affected records
- [ ] Stop writes if corruption spreading
- [ ] Restore from backup if needed
- [ ] Verify data integrity

### If caused by external dependency
- [ ] Enable fallback/degraded mode
- [ ] Communicate dependency status
- [ ] Monitor dependency status page
- [ ] Re-enable when dependency recovers

### If caused by traffic spike
- [ ] Scale up: `kubectl scale deployment/<name> --replicas=N`
- [ ] Enable rate limiting
- [ ] Identify traffic source
- [ ] Block abusive traffic if applicable

## Communication

- [ ] Update status page (if P1/P2)
- [ ] Notify affected users (if significant)
- [ ] Regular updates every 30 minutes during incident
- [ ] Final all-clear message when resolved

## Resolution

- [ ] Confirm service fully restored
- [ ] Verify no lingering errors
- [ ] Monitor for recurrence (30 minutes)
- [ ] Update status page to resolved
- [ ] Notify stakeholders of resolution

## Post-incident (within 48 hours)

- [ ] Schedule post-mortem (P1/P2 required)
- [ ] Collect timeline of events
- [ ] Gather logs, metrics, screenshots
- [ ] Identify root cause
- [ ] Document contributing factors
- [ ] Create follow-up tickets for fixes
- [ ] Write post-mortem document
- [ ] Share learnings with team

## Post-mortem template

```markdown
## Incident: [Title]
**Date:** YYYY-MM-DD
**Duration:** X hours Y minutes
**Severity:** P1/P2/P3/P4
**Commander:** [Name]

### Summary
One paragraph description.

### Timeline
- HH:MM - Event
- HH:MM - Event

### Root cause
What actually caused the incident.

### Contributing factors
What made it worse or harder to detect/resolve.

### What went well
- Item

### What could be improved
- Item

### Action items
- [ ] [Owner] Action (due date)
```

## Quick reference

### Useful commands
```bash
# Recent deployments
gh run list --limit 5

# Pod status
kubectl get pods -o wide

# Recent logs
kubectl logs -l app=tc-api --tail=100 --since=10m

# Events
kubectl get events --sort-by=.lastTimestamp

# Rollback
kubectl rollout undo deployment/tc-api

# Scale
kubectl scale deployment/tc-api --replicas=5
```

### Key contacts
- On-call: Check rotation schedule
- Infrastructure: [contact]
- Database: [contact]

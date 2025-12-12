# Postgres Backup and Point-in-Time Recovery

This document describes how to configure and manage PostgreSQL backups for the Tiny Congress identity system, including point-in-time recovery (PITR) capabilities.

## Overview

The identity system uses PostgreSQL to store:
- Sigchain events (append-only event log)
- Read models (accounts, devices, endorsements, etc.)
- Rate limiting data
- Session metadata

Proper backup and recovery procedures are critical for production deployments.

## Development Backups

### Manual Backup with pg_dump

For development and testing, use `pg_dump` to create logical backups:

```bash
# Backup entire database
pg_dump -h localhost -U postgres -d tinycongress > backup_$(date +%Y%m%d_%H%M%S).sql

# Backup with compression
pg_dump -h localhost -U postgres -d tinycongress | gzip > backup_$(date +%Y%m%d_%H%M%S).sql.gz

# Backup schema only (for migrations)
pg_dump -h localhost -U postgres -d tinycongress --schema-only > schema.sql

# Backup data only
pg_dump -h localhost -U postgres -d tinycongress --data-only > data.sql
```

### Restore from pg_dump

```bash
# Restore from backup
psql -h localhost -U postgres -d tinycongress < backup.sql

# Restore from compressed backup
gunzip -c backup.sql.gz | psql -h localhost -U postgres -d tinycongress

# Drop and recreate database before restore
dropdb -h localhost -U postgres tinycongress
createdb -h localhost -U postgres tinycongress
psql -h localhost -U postgres -d tinycongress < backup.sql
```

## Production Backups

### Continuous Archiving and PITR

PostgreSQL supports continuous archiving via Write-Ahead Logging (WAL). This enables point-in-time recovery to any moment in time.

**Configuration (postgresql.conf):**

```conf
# Enable WAL archiving
wal_level = replica
archive_mode = on
archive_command = 'test ! -f /mnt/backup/archive/%f && cp %p /mnt/backup/archive/%f'
archive_timeout = 300  # Force WAL rotation every 5 minutes

# Retention settings
wal_keep_size = 1GB
```

**Base Backup Creation:**

```bash
# Create base backup using pg_basebackup
pg_basebackup -h localhost -U postgres -D /mnt/backup/base -Fp -Xs -P

# With compression
pg_basebackup -h localhost -U postgres -D /mnt/backup/base -Fp -Xs -P -z
```

### Point-in-Time Recovery Process

1. **Stop PostgreSQL:**
   ```bash
   systemctl stop postgresql
   ```

2. **Restore base backup:**
   ```bash
   rm -rf /var/lib/postgresql/data/*
   cp -R /mnt/backup/base/* /var/lib/postgresql/data/
   ```

3. **Create recovery configuration:**
   ```bash
   cat > /var/lib/postgresql/data/recovery.signal <<EOF
   # This file triggers recovery mode
   EOF

   cat > /var/lib/postgresql/data/postgresql.auto.conf <<EOF
   restore_command = 'cp /mnt/backup/archive/%f %p'
   recovery_target_time = '2024-12-11 12:00:00'
   EOF
   ```

4. **Start PostgreSQL:**
   ```bash
   systemctl start postgresql
   ```

5. **Monitor recovery:**
   ```bash
   tail -f /var/lib/postgresql/data/log/postgresql.log
   ```

## Cloud Provider Solutions

### AWS RDS

AWS RDS provides automated backups with PITR:

```bash
# Create manual snapshot
aws rds create-db-snapshot \
  --db-instance-identifier tc-postgres \
  --db-snapshot-identifier tc-snapshot-$(date +%Y%m%d)

# Restore to point in time
aws rds restore-db-instance-to-point-in-time \
  --source-db-instance-identifier tc-postgres \
  --target-db-instance-identifier tc-postgres-restored \
  --restore-time "2024-12-11T12:00:00Z"
```

**Configuration:**
- Enable automated backups (retention: 7-35 days)
- Enable Multi-AZ for high availability
- Set backup window to low-traffic period
- Enable encryption at rest

### Google Cloud SQL

```bash
# Create backup
gcloud sql backups create \
  --instance=tc-postgres

# Restore backup
gcloud sql backups restore BACKUP_ID \
  --backup-instance=tc-postgres \
  --instance=tc-postgres-restored
```

**Configuration:**
- Enable automated daily backups
- Set backup window (e.g., 02:00-06:00 UTC)
- Configure transaction log retention (7 days recommended)
- Enable point-in-time recovery

### Azure Database for PostgreSQL

```bash
# Restore to point in time
az postgres server restore \
  --resource-group tc-rg \
  --name tc-postgres-restored \
  --restore-point-in-time "2024-12-11T12:00:00Z" \
  --source-server tc-postgres
```

**Configuration:**
- Backup retention: 7-35 days
- Geo-redundant backup for disaster recovery
- Point-in-time restore enabled by default

## Kubernetes with PersistentVolumeClaims

For Kubernetes deployments using StatefulSets with PVCs:

```bash
# Create VolumeSnapshot
kubectl apply -f - <<EOF
apiVersion: snapshot.storage.k8s.io/v1
kind: VolumeSnapshot
metadata:
  name: postgres-snapshot-$(date +%Y%m%d)
spec:
  volumeSnapshotClassName: csi-snapshot-class
  source:
    persistentVolumeClaimName: postgres-data
EOF

# Restore from snapshot
kubectl apply -f - <<EOF
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: postgres-data-restored
spec:
  dataSource:
    name: postgres-snapshot-20241211
    kind: VolumeSnapshot
    apiGroup: snapshot.storage.k8s.io
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 50Gi
EOF
```

## Backup Strategy Recommendations

### Frequency

- **Production:**
  - Full backup: Daily
  - WAL archiving: Continuous
  - Snapshots: Before major deployments

- **Staging:**
  - Full backup: Daily
  - Snapshots: Weekly

- **Development:**
  - Manual backups: As needed
  - Pre-migration snapshots: Always

### Retention

- **Production:**
  - Daily backups: 30 days
  - WAL archives: 7 days
  - Monthly backups: 1 year

- **Staging:**
  - Daily backups: 7 days

- **Development:**
  - Manual backups: Until no longer needed

### Testing

**Monthly backup drill:**
1. Restore latest backup to separate instance
2. Verify data integrity
3. Test application connectivity
4. Document recovery time objective (RTO)
5. Document recovery point objective (RPO)

## Monitoring and Alerts

### Backup Health Checks

```sql
-- Check last successful backup
SELECT pg_last_wal_replay_lsn(), pg_last_wal_receive_lsn();

-- Check WAL archive lag
SELECT
  CASE
    WHEN pg_last_wal_receive_lsn() = pg_last_wal_replay_lsn() THEN 0
    ELSE EXTRACT(EPOCH FROM now() - pg_last_xact_replay_timestamp())
  END AS replication_lag_seconds;
```

### Alert Conditions

- Backup fails 2 consecutive times
- WAL archive directory > 80% full
- Replication lag > 5 minutes
- Backup age > 25 hours

## Disaster Recovery Plan

1. **Assess situation:** Determine if PITR is needed or full restore
2. **Communicate:** Notify stakeholders of expected downtime
3. **Execute recovery:** Follow appropriate restore procedure
4. **Verify:** Check data integrity and application functionality
5. **Monitor:** Watch for replication lag and performance issues
6. **Document:** Record incident details and recovery time

## References

- [PostgreSQL PITR Documentation](https://www.postgresql.org/docs/current/continuous-archiving.html)
- [pg_basebackup Manual](https://www.postgresql.org/docs/current/app-pgbasebackup.html)
- Session key rotation: See `doc/secrets-rotation.md`
- Migration procedures: See `service/README.md`

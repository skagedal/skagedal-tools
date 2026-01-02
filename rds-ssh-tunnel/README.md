# rds-ssh-tunnel

A TypeScript tool for connecting to PostgreSQL databases via an SSH tunnel to a bastion host, optimized for AWS RDS.

## Features

- Creates SSH tunnel to bastion host using the ssh2 library
- Connects to PostgreSQL databases through the tunnel
- Optimized for AWS RDS databases
- Built with TypeScript and runs directly using Node.js native TypeScript support
- Comprehensive CLI argument parsing
- Optional database connection testing
- Graceful shutdown handling

## Prerequisites

- Node.js version 22.6.0 or higher (with `--experimental-strip-types` support)
- SSH access to a bastion host
- PostgreSQL database accessible from the bastion host

## Installation

```bash
cd rds-ssh-tunnel
npm install
```

## Usage

### Basic Usage

```bash
npm start -- \
  --bastion-host bastion.example.com \
  --db-host my-rds-instance.abc123.us-east-1.rds.amazonaws.com
```

### Full Example with All Options

```bash
npm start -- \
  --bastion-host bastion.example.com \
  --bastion-port 22 \
  --bastion-user ec2-user \
  --bastion-key ~/.ssh/my-key.pem \
  --db-host my-rds-instance.abc123.us-east-1.rds.amazonaws.com \
  --db-port 5432 \
  --db-user postgres \
  --db-password mypassword \
  --db-name mydatabase \
  --local-port 5433 \
  --test \
  --keep-alive
```

### Using Password Authentication (instead of SSH key)

```bash
npm start -- \
  --bastion-host bastion.example.com \
  --bastion-password mysshpassword \
  --db-host my-rds-instance.abc123.us-east-1.rds.amazonaws.com
```

### Direct Execution (with executable permissions)

```bash
chmod +x src/index.ts
./src/index.ts --bastion-host bastion.example.com --db-host my-db.rds.amazonaws.com
```

## CLI Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--bastion-host` | `-bh` | Bastion host address (required) | - |
| `--bastion-port` | `-bp` | Bastion SSH port | `22` |
| `--bastion-user` | `-bu` | Bastion SSH username | `$USER` or `ec2-user` |
| `--bastion-key` | `-bk` | Path to SSH private key | `~/.ssh/id_rsa` |
| `--bastion-password` | `-bpw` | SSH password (alternative to key) | - |
| `--db-host` | `-dh` | Database host/RDS endpoint (required) | - |
| `--db-port` | `-dp` | Database port | `5432` |
| `--db-user` | `-du` | Database username | `postgres` |
| `--db-password` | `-dpw` | Database password | - |
| `--db-name` | `-dn` | Database name | `postgres` |
| `--local-port` | `-lp` | Local port for tunnel | `5433` |
| `--test` | - | Test database connection | `false` |
| `--keep-alive` | - | Keep tunnel alive after test | `false` |

## AWS RDS Example

For an AWS RDS PostgreSQL instance accessed through an EC2 bastion host:

```bash
npm start -- \
  --bastion-host ec2-54-123-45-67.compute-1.amazonaws.com \
  --bastion-user ec2-user \
  --bastion-key ~/.ssh/aws-key.pem \
  --db-host myapp-db.c9akd8fjqk3l.us-east-1.rds.amazonaws.com \
  --db-user dbadmin \
  --db-password 'MySecurePassword123!' \
  --db-name production \
  --test \
  --keep-alive
```

This will:
1. Connect to the EC2 bastion host via SSH
2. Establish a tunnel from `localhost:5433` to the RDS instance
3. Test the PostgreSQL connection
4. Keep the tunnel alive for further use

You can then connect to the database using any PostgreSQL client:

```bash
psql postgresql://dbadmin@localhost:5433/production
```

## How It Works

1. **SSH Connection**: The tool establishes an SSH connection to your bastion host using either a private key or password
2. **Port Forwarding**: Creates a local port (default: 5433) that forwards traffic through the SSH tunnel
3. **Database Access**: Routes PostgreSQL traffic through the tunnel to your RDS instance
4. **Connection Testing**: Optionally tests the connection and displays database version info
5. **Keep Alive**: Can maintain the tunnel for ongoing use or exit after testing

## Security Notes

- Never commit passwords or private keys to version control
- Use environment variables or secure credential management for sensitive data
- Consider using AWS Systems Manager Session Manager as an alternative to traditional bastion hosts
- Ensure your security groups allow PostgreSQL traffic from the bastion host to RDS

## Troubleshooting

### SSH Connection Fails

- Verify bastion host address and SSH credentials
- Check that your SSH key has proper permissions (`chmod 600 ~/.ssh/key.pem`)
- Ensure security groups allow SSH (port 22) to bastion host

### Database Connection Fails

- Verify RDS endpoint is correct
- Check that RDS security group allows PostgreSQL (port 5432) from bastion host
- Confirm database credentials are correct
- Ensure RDS instance is in a state that accepts connections

### Tunnel Closes Unexpectedly

- Check for network connectivity issues
- Verify SSH session timeout settings on bastion host
- Consider implementing keep-alive packets for long-running tunnels

## License

ISC

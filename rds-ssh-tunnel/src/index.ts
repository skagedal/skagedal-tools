#!/usr/bin/env node --experimental-strip-types

import { Client } from 'ssh2';
import { Client as PgClient } from 'pg';
import { Command } from 'commander';
import * as net from 'net';
import * as fs from 'fs';

interface TunnelConfig {
    // SSH Bastion configuration
    bastionHost: string;
    bastionPort: number;
    bastionUser: string;
    bastionKeyPath?: string;
    bastionPassword?: string;

    // Database configuration
    dbHost: string;
    dbPort: number;
    dbUser: string;
    dbPassword?: string;
    dbName: string;

    // Local tunnel configuration
    localPort: number;
}

class SSHTunnel {
    private sshClient: Client;
    private server: net.Server | null = null;
    private config: TunnelConfig;

    constructor(config: TunnelConfig) {
        this.config = config;
        this.sshClient = new Client();
    }

    async connect(): Promise<void> {
        return new Promise((resolve, reject) => {
            console.log(`Connecting to bastion host ${this.config.bastionUser}@${this.config.bastionHost}:${this.config.bastionPort}...`);

            const sshConfig: any = {
                host: this.config.bastionHost,
                port: this.config.bastionPort,
                username: this.config.bastionUser,
            };

            if (this.config.bastionKeyPath) {
                try {
                    sshConfig.privateKey = fs.readFileSync(this.config.bastionKeyPath);
                } catch (error) {
                    reject(new Error(`Failed to read SSH key from ${this.config.bastionKeyPath}: ${error}`));
                    return;
                }
            } else if (this.config.bastionPassword) {
                sshConfig.password = this.config.bastionPassword;
            } else {
                reject(new Error('Either bastionKeyPath or bastionPassword must be provided'));
                return;
            }

            this.sshClient
                .on('ready', () => {
                    console.log('SSH connection established');
                    this.setupTunnel();
                    resolve();
                })
                .on('error', (err) => {
                    reject(new Error(`SSH connection error: ${err.message}`));
                })
                .connect(sshConfig);
        });
    }

    private setupTunnel(): void {
        this.server = net.createServer((sock) => {
            console.log(`New connection on local port ${this.config.localPort}`);

            this.sshClient.forwardOut(
                sock.remoteAddress!,
                sock.remotePort!,
                this.config.dbHost,
                this.config.dbPort,
                (err, stream) => {
                    if (err) {
                        console.error('Error forwarding connection:', err);
                        sock.end();
                        return;
                    }

                    sock.pipe(stream);
                    stream.pipe(sock);

                    sock.on('error', (error) => {
                        console.error('Socket error:', error);
                    });

                    stream.on('error', (error) => {
                        console.error('Stream error:', error);
                    });
                }
            );
        });

        this.server.listen(this.config.localPort, '127.0.0.1', () => {
            console.log(`SSH tunnel established: localhost:${this.config.localPort} -> ${this.config.dbHost}:${this.config.dbPort}`);
        });
    }

    async close(): Promise<void> {
        return new Promise((resolve) => {
            if (this.server) {
                this.server.close(() => {
                    console.log('Tunnel server closed');
                    this.sshClient.end();
                    resolve();
                });
            } else {
                this.sshClient.end();
                resolve();
            }
        });
    }
}

async function testDatabaseConnection(config: TunnelConfig): Promise<void> {
    const client = new PgClient({
        host: '127.0.0.1',
        port: config.localPort,
        user: config.dbUser,
        password: config.dbPassword,
        database: config.dbName,
    });

    try {
        console.log('\nTesting database connection...');
        await client.connect();
        console.log('Successfully connected to PostgreSQL database!');

        const result = await client.query('SELECT version()');
        console.log('\nDatabase version:');
        console.log(result.rows[0].version);

        const dbResult = await client.query('SELECT current_database()');
        console.log(`\nConnected to database: ${dbResult.rows[0].current_database}`);

        await client.end();
    } catch (error) {
        console.error('Database connection failed:', error);
        throw error;
    }
}

async function main() {
    const program = new Command();

    program
        .name('rds-ssh-tunnel')
        .description('Connect to PostgreSQL databases via SSH tunnel to a bastion host, optimized for AWS RDS')
        .version('1.0.0');

    program
        .option('-bh, --bastion-host <host>', 'Bastion host address')
        .option('-bp, --bastion-port <port>', 'Bastion SSH port', '22')
        .option('-bu, --bastion-user <user>', 'Bastion SSH username', process.env.USER || 'ec2-user')
        .option('-bk, --bastion-key <path>', 'Path to SSH private key', `${process.env.HOME}/.ssh/id_rsa`)
        .option('-bpw, --bastion-password <password>', 'Bastion SSH password (alternative to key)')
        .option('-dh, --db-host <host>', 'Database host (RDS endpoint)')
        .option('-dp, --db-port <port>', 'Database port', '5432')
        .option('-du, --db-user <user>', 'Database username', 'postgres')
        .option('-dpw, --db-password <password>', 'Database password')
        .option('-dn, --db-name <name>', 'Database name', 'postgres')
        .option('-lp, --local-port <port>', 'Local port for tunnel', '5433')
        .option('--test', 'Test the database connection after establishing tunnel', false)
        .option('--keep-alive', 'Keep the tunnel alive (default: exit after test)', false);

    program.parse();

    const options = program.opts();

    // Validate required options
    if (!options.bastionHost) {
        console.error('Error: --bastion-host is required');
        process.exit(1);
    }

    if (!options.dbHost) {
        console.error('Error: --db-host is required');
        process.exit(1);
    }

    const config: TunnelConfig = {
        bastionHost: options.bastionHost,
        bastionPort: parseInt(options.bastionPort),
        bastionUser: options.bastionUser,
        bastionKeyPath: options.bastionPassword ? undefined : options.bastionKey,
        bastionPassword: options.bastionPassword,
        dbHost: options.dbHost,
        dbPort: parseInt(options.dbPort),
        dbUser: options.dbUser,
        dbPassword: options.dbPassword,
        dbName: options.dbName,
        localPort: parseInt(options.localPort),
    };

    const tunnel = new SSHTunnel(config);

    // Handle graceful shutdown
    const cleanup = async () => {
        console.log('\n\nShutting down...');
        await tunnel.close();
        process.exit(0);
    };

    process.on('SIGINT', cleanup);
    process.on('SIGTERM', cleanup);

    try {
        await tunnel.connect();

        if (options.test) {
            await testDatabaseConnection(config);

            if (!options.keepAlive) {
                console.log('\nTest complete. Use --keep-alive to keep the tunnel open.');
                await cleanup();
            } else {
                console.log('\nTunnel is active. Press Ctrl+C to close.');
                console.log(`\nConnection string: postgresql://${config.dbUser}@localhost:${config.localPort}/${config.dbName}`);
            }
        } else {
            console.log('\nTunnel is active. Press Ctrl+C to close.');
            console.log(`\nConnection string: postgresql://${config.dbUser}@localhost:${config.localPort}/${config.dbName}`);
        }

        // Keep the process alive if not testing or if keep-alive is set
        if (!options.test || options.keepAlive) {
            await new Promise(() => {}); // Keep alive indefinitely
        }
    } catch (error) {
        console.error('Error:', error);
        process.exit(1);
    }
}

main();

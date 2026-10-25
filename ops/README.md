# Operação de uma instância

Exemplos para uma VPS por país. Valores como `<subdominio>` são placeholders; nada aqui foi validado em uma VPS real.

## Implantação

1. `just release` gera `target/release/dwc`, `target/release/hash.txt` e `target/site/`.
2. Copie para `/opt/dwc/releases/<versão>/`: `dwc`, `hash.txt` (sempre ao lado do binário; sem ele os nomes com hash de `/pkg` não são encontrados), `site/`, `catalog/` e `ops/`. Aponte `/opt/dwc/current` para a release.
3. Crie o usuário `dwc`, `/etc/dwc/dwc.env` a partir de [`dwc.env.example`](dwc.env.example) (modo 0600), a chave em `DWC_SECRET_KEY_FILE` (32 bytes ou 64 hex, modo 0600, dono `dwc`; em produção outra permissão impede a inicialização) e `/var/lib/dwc/snapshots` (dono `dwc`).
4. Instale [`systemd/dwc.service`](systemd/dwc.service) e o proxy [`caddy/Caddyfile`](caddy/Caddyfile). A aplicação escuta só em loopback.

## Primeiro Admin País

O motor TursoDB trava o arquivo do banco para o processo dono. Comandos de operador rodam com o serviço parado, com o mesmo ambiente:

```sh
systemctl stop dwc
sudo -u dwc sh -c 'set -a; . /etc/dwc/dwc.env; cd /opt/dwc/current && ./dwc grant-admin <email>'
systemctl start dwc
```

A conta precisa existir e estar ativa. O evento fica na auditoria (`actor_ref = operator:cli`). Auditores são credenciados depois pelo painel `/admin/auditores`.

## Flags

O painel `/admin` grava flags e mostra "pendente de reinício". O painel não reinicia o serviço; o operador executa `systemctl restart dwc` (parada graciosa de até 30 s, `TimeoutStopSec=35`).

## Atualização e rollback

- Pare o serviço, troque o link `current` e inicie; as migrations novas rodam na partida.
- Rollback só para um binário que conheça todas as migrations aplicadas: um binário antigo recusa um banco com migration desconhecida em vez de divergir. Não há downgrade nem reset automático do banco; para voltar o schema, restaure um backup.
- Faça um backup offline (`ops/backup.sh --offline`, serviço parado) antes de cada atualização.

## Backup e restauração

- Online: com `DWC_SNAPSHOT_DIR` configurado, `SIGUSR1` faz o próprio processo gravar uma cópia consistente com `VACUUM INTO` do motor. [`backup.sh --online <pid>`](backup.sh) usa isso, verifica a cópia com `dwc check-db`, criptografa com `age` (só chaves públicas na VPS) e mantém 7 diários e 4 semanais.
- Offline: `backup.sh --offline` usa `dwc backup` com o serviço parado.
- Agendamento: [`systemd/dwc-backup.timer`](systemd/dwc-backup.timer) diário. Copie `/var/backups/dwc` para armazenamento separado.
- Teste de restauração: `DWC_BACKUP_IDENTITY=<chave privada> ops/restore-test.sh <arquivo.db.age>` descriptografa em diretório temporário, roda `dwc check-db` (integridade, migrations, cadeia de auditoria, contagens) e compara com o manifesto do backup. `just backup-test` executa o ciclo completo com banco descartável.

Limites atuais: só há camada diária (RPO de 1 hora exige uma camada horária com retenção a definir); a cópia completa a cada execução não é incremental; RTO de 4 horas não foi medido em restauração real.

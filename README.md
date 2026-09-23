# 🔥 flare-cli (`flare`)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust: 2021](https://img.shields.io/badge/Rust-2021%20Edition-orange.svg)](https://www.rust-lang.org/)
[![Architecture: Agentless](https://img.shields.io/badge/Architecture-Agentless%20SSH-success.svg)](#zero-agent-philosophy)
[![Docker & Dokploy](https://img.shields.io/badge/Supports-Docker%20%26%20Dokploy-blue.svg)](https://dokploy.com/)

**Flare** est un outil CLI DevOps ultra-léger et **agentless** conçu pour administrer, inspecter et surveiller en temps réel une flotte de serveurs distants Linux exécutant **Docker** et **Dokploy**, sans installer de démon permanent ou d'agent lourd sur les machines cibles.

---

## ⚡ Points forts & Philosophie

- **Zéro Agent (*Agentless*) :** Aucune dépendance ni agent d'arrière-plan à maintenir sur vos serveurs distants. Une simple connexion SSH suffit.
- **Transport SSH Natif :** Exploite directement votre configuration SSH locale (`~/.ssh/config`, `ssh-agent`, clés sécurisées FIDO/ED25519, ProxyJump).
- **Sonde distante composite :** Collecte en un seul aller-retour SSH l'ensemble des métriques d'un serveur (CPU, RAM, Disque racine, GPU NVIDIA et conteneurs Docker/Dokploy). Sonde basée sur Python 3 standard avec fallback automatique en shell POSIX + Awk.
- **Tableau de bord multi-serveurs :** Interroge vos nœuds **en parallèle** pour afficher une synthèse instantanée ou inspecter un serveur en profondeur.
- **Mode Sentinelle (*Alerting & Cron*) :** Évalue la santé de vos nœuds par rapport à des seuils configurables, intègre un cache anti-spam local (*cooldown*) et expédie des alertes vers **[ntfy.sh](https://ntfy.sh)**, des salons **Discord** ou des webhooks personnalisés.

---

## 🚀 Installation

### Prérequis
- **Machine locale (opérateur) :** Rust 1.80+ (édition 2021) et le client `ssh` OpenSSH installé.
- **Serveurs cibles :** Nœud Linux avec accès SSH (utilisateur avec droits Docker). Python 3 recommandé (bibliothèque standard uniquement), ou environnement shell POSIX.

### Compilation & Installation
Clonez le dépôt et compilez le binaire :

```bash
git clone https://github.com/votre-compte/flare.git
cd flare

# Compiler le binaire de production
cargo build --release

# Installer directement dans ~/.cargo/bin (accessible dans votre PATH)
cargo install --path .
```

Le binaire exécutable est nommé **`flare`**.

---

## 📖 Utilisation

```text
Usage: flare <COMMAND>

Commands:
  server  Manage remote servers in the local inventory
  ssh     Open an interactive SSH shell to a managed server
  status  Collect and display real-time metrics (CPU, RAM, Disk, GPU, Docker)
  check   Sentinel health check & alerting (designed for cron or manual triggers)
  help    Print this message or the help of the given subcommand(s)
```

---

### 1. Gestion de l'inventaire des serveurs (`flare server`)

#### Ajouter un serveur (`flare server add`)
La commande propose un assistant interactif pas-à-pas avec test de connectivité SSH automatique :

```bash
flare server add
```

Vous pouvez également passer directement les arguments pour automatiser l'ajout :

```bash
flare server add \
  --id "prod-dokploy" \
  --name "Dokploy Production" \
  --host "vps.example.com" \
  --port 22 \
  --user "root" \
  --key-path "~/.ssh/id_ed25519" \
  --tags "prod,dokploy"
```

#### Lister les serveurs enregistrés (`flare server list`)
Affiche un tableau récapitulatif avec test de latence et disponibilité en parallèle :

```bash
flare server list

# Filtrer par tag
flare server list --tag prod
```

#### Supprimer un serveur (`flare server remove`)
```bash
flare server remove [server_id]
```

---

### 2. Accès SSH interactif (`flare ssh`)

Ouvre une session shell interactive avec allocation pseudo-terminal (`-t`) :

```bash
# Connexion directe par identifiant
flare ssh prod-dokploy

# Si aucun identifiant n'est fourni, un menu de sélection interactif s'affiche
flare ssh
```

---

### 3. Tableau de bord & Métriques en temps réel (`flare status`)

#### Vue de synthèse multi-serveurs (en parallèle)
Si aucun serveur n'est spécifié, `flare` interroge l'intégralité de vos serveurs de façon asynchrone et concurrente :

```bash
flare status
```

Exemple de sortie :
```text
• Gathering real-time metrics across 2 server(s) in parallel...
╭────────────────────────┬──────────────┬───────┬───────┬──────────────────┬──────────────────┬──────────────┬──────────────────┬────────╮
│ SERVER                 ┆ HOST         ┆ PROBE ┆ CPU % ┆ RAM % (USED/TOT) ┆ DISK % (USED/TOT)┆ GPU          ┆ DOCKER (R/R/S)   ┆ STATUS │
╞════════════════════════╪══════════════╪═══════╪═══════╪══════════════════╪══════════════════╪══════════════╪══════════════════╪════════╡
│ Dokploy (prod-dokploy) ┆ 10.0.0.1:22  ┆ 142ms ┆ 12.4% ┆ 48.2% (7.7/16.0G)┆ 34.0% (34/100G)  ┆ RTX 4060 15% ┆ 12/0/1           ┆ ● OK   │
│ Staging (staging-node) ┆ 10.0.0.2:22  ┆ 98ms  ┆ 4.1%  ┆ 22.0% (3.5/16.0G)┆ 18.0% (18/100G)  ┆ -            ┆ 4/0/0            ┆ ● OK   │
╰────────────────────────┴──────────────┴───────┴───────┴──────────────────┴──────────────────┴──────────────┴──────────────────┴────────╯
```

#### Vue détaillée d'un serveur unique
Affiche un rapport complet incluant les jauges de charge, la mémoire, le disque, le GPU et le détail de chaque conteneur Docker :

```bash
flare status prod-dokploy
```

#### Export JSON
Idéal pour intégrer `flare` dans d'autres scripts ou exporter vers Prometheus / Grafana :

```bash
flare status --json
flare status prod-dokploy --json
```

---

### 4. Mode Sentinelle & Alerting (`flare check`)

Conçu pour une exécution manuelle ou via une tâche planifiée locale (**cron** ou **systemd timer**) :
- Analyse l'état de chaque serveur.
- Détecte :
  - L'inaccessibilité de l'hôte (Critical)
  - Le dépassement des seuils CPU, RAM, Disque (Warning / Critical si $\ge 95\%$)
  - Les conteneurs Docker en boucle de crash / redémarrage perpétuel (Critical)
  - Les conteneurs Docker arrêtés ou sortis en erreur (Warning)
- Évite les tempêtes de notifications grâce à son cache local dédupliqué (`cooldown_minutes`).
- Émet vers **ntfy.sh**, **Discord** ou un webhook générique.

```bash
# Vérification standard
flare check

# Mode simulation (n'envoie aucun webhook)
flare check --dry-run

# Forcer l'envoi des alertes en ignorant le cooldown cache
flare check --force

# Vérifier uniquement une catégorie de serveurs
flare check --tag dokploy
```

#### Configuration Cron recommandée (ex. toutes les 10 minutes)
```cron
*/10 * * * * /home/user/.cargo/bin/flare check >> /home/user/.cache/flare/check.log 2>&1
```

---

## ⚙️ Configuration (`~/.config/flare/config.yaml`)

Le fichier de configuration est automatiquement créé lors de la première utilisation :

```yaml
# ~/.config/flare/config.yaml
servers:
  - id: prod-dokploy
    name: Production Main VPS
    host: 198.51.100.10
    port: 22
    user: root
    key_path: ~/.ssh/id_ed25519
    tags:
      - prod
      - dokploy

  - id: backup-node
    name: Backup & Storage Node
    host: 198.51.100.11
    port: 2222
    user: admin
    tags:
      - backup

alerts:
  # Topic ntfy.sh, webhook Discord, ou endpoint générique
  webhook_url: https://ntfy.sh/mon_topic_secret_flare
  cpu_percent: 85.0
  ram_percent: 90.0
  disk_percent: 85.0
  notify_stopped_containers: true
  notify_restarting_containers: true
  # Durée en minutes avant répétition d'une même alerte
  cooldown_minutes: 60
```

---

## 🛡️ Cache Sentinelle Anti-Spam

Pour éviter de saturer vos canaux de messagerie lorsqu'un serveur reste en alerte, `flare` enregistre les alertes déclenchées dans `~/.cache/flare/alert_cache.json`.
Une alerte identique n'est réémise qu'après l'expiration de la fenêtre de `cooldown_minutes` (par défaut 60 minutes), ou immédiatement si vous spécifiez le drapeau `--force`.

---

## 🧪 Tests de développement

Pour exécuter la suite de tests unitaires et d'intégration :

```bash
cargo test
```

Pour vérifier le formatage et l'absence d'avertissements :

```bash
cargo check
```

Consultez le fichier [`AGENTS.md`](AGENTS.md) pour les détails d'architecture, la carte des modules et les directives de contribution.

---

## 📄 Licence

Ce projet est sous licence libre [MIT](LICENSE). Développé par **Simon Micheneau** (<contact@simon-micheneau.fr>).

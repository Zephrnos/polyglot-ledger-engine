# Dev Plan

## Phase 1

Goal: Prove the core business logic works on your local machine.

- [ ] Build Go API
- [ ] Build Rust Core
- [ ] Connect to PostgreSQL
- [ ] Connect to RabbitMQ
- [ ] Test the basic flow: API receives -> publishes to queue -> Rust consumes -> writes to DB.

## Phase 2

Goal: Move the working MVP onto a basic Kubernetes cluster. Only 1 replica each.

- [ ] Dockerize Go API
- [ ] Dockerize Rust Core

Create K3 deployment and service files for:

- [ ] Go API
- [ ] Rust Core
- [ ] RabbitMQ
- [ ] PostgreSQL

## Phase 3

Goal: Add the remaining features directly into the Kubernetes environment.

- [ ] Build Python Fraud Service
- [ ] Dockerize Python Fraud Service
- [ ] Create K3 Deployment and Service files
- [ ] Deploy and test with 1 replicas
- [ ] Add Prometheus endpoint to get metrics

## Phase 4

Goal: Make the system highly available and scalable.

- [ ] Increase replica counts
- [ ] Setup PostgreSQL Replication

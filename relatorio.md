### Relatório de Implementação do Protocolo de Transferência de Arquivos
Gabriel Avila, Carlos Botelho

#### **Estratégias de Implementação**
O protocolo foi desenvolvido utilizando uma abordagem **stateful**, com as seguintes estratégias principais:
- **Offset Tracking**: Implementação de transferência de arquivos com suporte à retomada, onde o cliente e o servidor mantêm controle do byte offset para continuar downloads/uploads interrompidos.
- **Chunked Transfers**: Transferência de dados em blocos, utilizando comandos como `NEXT <chunk_size>` para otimizar o uso de banda e melhorar a resiliência.
- **Concurrency Control**: Controle de clientes simultâneos por meio de um contador global (`ACTIVE_CLIENTS`) e limitação configurável de conexões (`MAX_CLIENTS`).
- **Session Context**: Dependência de contexto durante as interações, como na comunicação de comandos `GET` e `PUT`, garantindo sincronização e consistência nos dados transferidos.

---

#### **Como Rodar**
1. Instale o gerenciador de pacotes e o compilador do Rust:
   `apt-get install cargo`
2. Atualize os pacotes do workspace:
   `cargo update --workspace`
3. Compile o código em modo de produção:
   `cargo build --workspace --release`
4. Os binários gerados podem ser encontrados no diretório `./target/release/` com os seguintes nomes:
   - `remcp-serv.exe`: Servidor de transferência de arquivos.
   - `remcp.exe`: Cliente de transferência de arquivos.

5. **Parâmetros disponíveis no servidor**:
   - `--debug`: Ativa o modo de depuração.
   - `--max-clients <número>`: Define o número máximo de clientes simultâneos. O valor padrão é `5`.
   - `--transfer-rate <taxa>`: Define a taxa máxima de transferência em bytes por segundo. O valor padrão é `256`.

6. **Parâmetros disponíveis no cliente**:
   - `--debug`: Ativa o modo de depuração.
   - `<source>` e `<destination>`: Caminhos para os arquivos ou diretórios.
     - O parâmetro `source` ou `destination` pode ser remoto, identificado pela presença de `:` no caminho.

7. **Instruções para o servidor**:
   - Escolha uma pasta onde o servidor (`remcp-serv.exe`) será executado.
   - O servidor é executado em background. Para verificar sua execução, utilize:
     `ps -aux | grep remcp-serv`

---

#### **Exemplos de Saídas com Diferentes Parametrizações**
1. **Transferência de Arquivos Pequenos**:
   - Com valores baixos de `TRANSFER_RATE`, observa-se um comportamento estável, com controle eficiente de banda.
   - Exemplo de saída: 
     `GET operation completed successfully. File received in chunks of 256 bytes.`

2. **Transferência de Arquivos Grandes**:
   - Com muitos clientes simultâneos (`MAX_CLIENTS` elevado), o protocolo aplica rate limiting para evitar sobrecarga do servidor.
   - Exemplo de saída:
     `Rate limited to 128 bytes per client. Download completed after 5 retries due to "Server is busy."`

---

#### **Discussão das Conclusões ao Variar Parâmetros**
- **`TRANSFER_RATE` elevado no servidor**:
  - Ao configurar um valor muito alto para `TRANSFER_RATE`, é possível que ocorra o estouro de buffers devido ao usize usado para gerar o buffer.

- **`MAX_CLIENTS` baixo**:
  - Reduzindo o número máximo de clientes simultâneos, o servidor prioriza estabilidade e evita sobrecarga, mas limita a escalabilidade.

- **Offset e Chunk Size**:
  - A granularidade dos chunks influencia diretamente o desempenho. Valores muito baixos aumentam a latência devido à troca frequente de comandos.

---

### **Conclusão**
A implementação oferece um protocolo eficiente e robusto para transferência de arquivos, com suporte a retomadas e controle de carga. No entanto, a configuração inadequada de parâmetros como `TRANSFER_RATE` pode levar a problemas de estabilidade, reforçando a importância de um balanceamento adequado para cada cenário de uso.

use crate::*;
use ethers::prelude::*;
use polars::prelude::*;

/// columns for geth traces
#[cryo_to_df::to_df(Datatype::GethTraces)]
#[derive(Default)]
pub struct GethTraces {
    n_rows: u64,
    typ: Vec<String>,
    from_address: Vec<Vec<u8>>,
    to_address: Vec<Option<Vec<u8>>>,
    value: Vec<Option<U256>>,
    gas: Vec<U256>,
    gas_used: Vec<U256>,
    input: Vec<Vec<u8>>,
    output: Vec<Option<Vec<u8>>>,
    error: Vec<Option<String>>,
    block_number: Vec<Option<u32>>,
    transaction_hash: Vec<Option<Vec<u8>>>,
    transaction_index: Vec<u32>,
    trace_address: Vec<String>,
    chain_id: Vec<u64>,
}

#[async_trait::async_trait]
impl Dataset for GethTraces {}

#[async_trait::async_trait]
impl CollectByBlock for GethTraces {
    type Response = (Option<u32>, Vec<Option<Vec<u8>>>, Vec<CallFrame>);

    async fn extract(request: Params, source: Arc<Source>, query: Arc<Query>) -> R<Self::Response> {
        let schema = query.schemas.get_schema(&Datatype::GethTraces)?;
        let include_transaction = schema.has_column("block_number");
        let block_number = request.block_number()? as u32;
        source.fetcher.geth_debug_trace_block_calls(block_number, include_transaction).await
    }

    fn transform(response: Self::Response, columns: &mut Self, query: &Arc<Query>) -> R<()> {
        process_geth_traces(response, columns, &query.schemas)
    }
}

#[async_trait::async_trait]
impl CollectByTransaction for GethTraces {
    type Response = (Option<u32>, Vec<Option<Vec<u8>>>, Vec<CallFrame>);

    async fn extract(request: Params, source: Arc<Source>, query: Arc<Query>) -> R<Self::Response> {
        let schema = query.schemas.get_schema(&Datatype::GethTraces)?;
        let include_block_number = schema.has_column("block_number");
        source
            .fetcher
            .geth_debug_trace_transaction_calls(request.transaction_hash()?, include_block_number)
            .await
    }

    fn transform(response: Self::Response, columns: &mut Self, query: &Arc<Query>) -> R<()> {
        process_geth_traces(response, columns, &query.schemas)
    }
}

fn process_geth_traces(
    traces: (Option<u32>, Vec<Option<Vec<u8>>>, Vec<CallFrame>),
    columns: &mut GethTraces,
    schemas: &Schemas,
) -> R<()> {
    let (block_number, txs, traces) = traces;
    let schema = schemas.get(&Datatype::GethTraces).ok_or(err("schema for geth_traces missing"))?;
    for (tx_index, (tx, trace)) in txs.into_iter().zip(traces).enumerate() {
        process_trace(trace, columns, schema, &block_number, &tx, tx_index as u32, vec![])?
    }
    Ok(())
}

fn process_trace(
    trace: CallFrame,
    columns: &mut GethTraces,
    schema: &Table,
    block_number: &Option<u32>,
    tx: &Option<Vec<u8>>,
    tx_index: u32,
    trace_address: Vec<u32>,
) -> R<()> {
    columns.n_rows += 1;
    store!(schema, columns, typ, trace.typ);
    store!(schema, columns, from_address, trace.from.as_bytes().to_vec());
    store!(schema, columns, to_address, noa_to_vec_u8(trace.to)?);
    store!(schema, columns, value, trace.value);
    store!(schema, columns, gas, trace.gas);
    store!(schema, columns, gas_used, trace.gas_used);
    store!(schema, columns, input, trace.input.0.to_vec());
    store!(schema, columns, output, trace.output.map(|x| x.0.to_vec()));
    store!(schema, columns, error, trace.error);
    store!(schema, columns, block_number, *block_number);
    store!(schema, columns, transaction_hash, tx.clone());
    store!(schema, columns, transaction_index, tx_index);
    store!(
        schema,
        columns,
        trace_address,
        trace_address.iter().map(|&n| n.to_string()).collect::<Vec<_>>().join(" ")
    );

    if let Some(subcalls) = trace.calls {
        for (s, subcall) in subcalls.into_iter().enumerate() {
            let mut sub_trace_address = trace_address.clone();
            sub_trace_address.push(s as u32);
            process_trace(subcall, columns, schema, block_number, tx, tx_index, sub_trace_address)?
        }
    }

    Ok(())
}

fn noa_to_vec_u8(value: Option<NameOrAddress>) -> R<Option<Vec<u8>>> {
    match value {
        Some(NameOrAddress::Address(address)) => Ok(Some(address.as_bytes().to_vec())),
        Some(NameOrAddress::Name(_)) => Err(err("block name string not allowed")),
        None => Ok(None),
    }
}

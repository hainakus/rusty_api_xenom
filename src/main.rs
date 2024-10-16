

use std::collections::HashMap;
use std::hash::Hash;
use actix_web::{web, App, HttpServer, Responder, HttpResponse};
use serde::{Deserialize, Serialize};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::RpcAddress;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use kaspa_wrpc_client::prelude::ConnectOptions;
use tokio::time::Duration;
use anyhow::Context;
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
use serde_json::json;
use chrono::{Utc, TimeZone};
use futures_util::future::err;

// Add this import
#[derive(Debug, Serialize, Deserialize)]
struct BalanceResponse {
    balance: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(web::resource("/blocks/{hash}").route(web::get().to(get_block)))
            .service(web::resource("/info/blockreward").route(web::get().to(get_block_reward)))
            .service(web::resource("/transactions/{hash}").route(web::get().to(get_transaction)))
            .service(web::resource("/info/blockdag").route(web::get().to(get_block_dag_info)))
            .service(web::resource("/info/kaspad").route(web::get().to(get_kaspad_info)))
            .service(web::resource("/info/hashrate/max").route(web::get().to(get_max_hashrate)))
            .service(web::resource("/info/coinsupply").route(web::get().to(get_coin_supply)))
            .service(web::resource("/addresses/{addr}/balance").route(web::get().to(get_balance_by_address)))
            .service(web::resource("/info/halving").route(web::get().to(get_halving)))
    })
        .bind(("127.0.0.1", 3001))?
        .run()
        .await
}


async fn get_client() -> Result<KaspaRpcClient, HttpResponse> {
    // Create a new KaspaRpcClient
    let mut kaspa_rpc = KaspaRpcClient::new(
        WrpcEncoding::SerdeJson,
        Some("ws://eu.losmuchachos.digital:19910"),
        None,
        None,
        None,
    ).unwrap();

    // Define the connection options
    let connect_options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(5)),
        ..Default::default()
    };

    // Attempt to connect with proper error handling
    kaspa_rpc
        .connect(Some(connect_options))
        .await
        .with_context(|| "Failed to connect to Kaspa node")
        .map_err(|_| {
            HttpResponse::InternalServerError().json("Failed to connect to Kaspa node")
        })?;

    // Return the connected client
    Ok(kaspa_rpc)
}
async fn disconnect_client(client: web::Data<KaspaRpcClient>) -> HttpResponse {
    match client.disconnect().await {
        Ok(_) => HttpResponse::Ok().body("Client disconnected successfully"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed to disconnect: {}", e)),
    }
}

async fn get_block(path: web::Path<String>) -> impl Responder {
    let hash = path.into_inner();
    let mut client = match get_client().await {
        Ok(client) => client,
        Err(err) => return err,
    };

   let block = match client.get_block(hash.parse().unwrap(), true).await {
        Ok(block) => block,
      Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get block DAG info: {:?}", err));
        }
    };
    // Ensure the client disconnects after the request
    if let Err(disconnect_err) = client.disconnect().await {
        // Log or handle disconnect error if needed
        return HttpResponse::InternalServerError().json(format!("Failed to disconnect: {}", disconnect_err));
    }

    HttpResponse::Ok().json(block)
}

async fn get_block_reward() -> impl Responder {
    // Step 1: Get the Kaspa RPC client
    let mut client = match get_client().await {
        Ok(client) => client,
        Err(err) => return err,
    };

    // Step 2: Get the latest block's hash from the DAG info
    let block_dag_info = match client.get_block_dag_info().await {
        Ok(info) => info,
        Err(_) => return HttpResponse::InternalServerError().json("Failed to fetch block DAG info"),
    };
    let latest_block_hash = block_dag_info.tip_hashes[0].clone();

    // Step 3: Get the latest block using its hash, including transactions
    let block_result = match client.get_block(latest_block_hash, true).await {
        Ok(block) => block,
        Err(_) => return HttpResponse::InternalServerError().json("Failed to fetch block information"),
    };

    // Step 4: Extract the coinbase transaction (the first transaction in the block)
    let coinbase_tx = match block_result.transactions.get(0) {
        Some(tx) => tx,
        None => return HttpResponse::InternalServerError().json("No coinbase transaction found"),
    };
    // Ensure the client disconnects after the request
    if let Err(disconnect_err) = client.disconnect().await {
        // Log or handle disconnect error if needed
        return HttpResponse::InternalServerError().json(format!("Failed to disconnect: {}", disconnect_err));
    }
    // Step 5: Calculate the block reward by summing the outputs
    let mut block_reward: u64 = 0;  // Use `u64` for reward in smallest unit

    for output in &coinbase_tx.outputs {
        println!("The block reward is not stable at 500 XEN, it is: {}", output.value);
        block_reward = output.value;
    }
    let block_reward_in_xen = block_reward as f64 / SOMPI_PER_KASPA as f64;
    // Step 6: Respond with the block reward and block hash
    HttpResponse::Ok().json(json!({
        "block_hash": latest_block_hash,
        "block_reward": block_reward_in_xen
    }))

}

async fn get_transaction(path: web::Path<String>) -> impl Responder {
    let hash = path.into_inner();
    let mut client = match get_client().await {
        Ok(client) => client,
        Err(err) => return err,
    };

  let transaction =  match client.get_block(hash.parse().unwrap(), true).await {
        Ok(transaction) => transaction,
      Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get block DAG info: {:?}", err));
        }
    };
    // Ensure the client disconnects after the request
    if let Err(disconnect_err) = client.disconnect().await {
        // Log or handle disconnect error if needed
        return HttpResponse::InternalServerError().json(format!("Failed to disconnect: {}", disconnect_err));
    }
    HttpResponse::Ok().json(transaction)


}

async fn get_block_dag_info() -> impl Responder {
    let mut client = match get_client().await {
        Ok(client) => client,
        Err(err) => return err,
    };

    let info = match client.get_block_dag_info().await {
        Ok(info) => info,
      Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get block DAG info: {:?}", err));
        }
    };
    // Ensure the client disconnects after the request
    if let Err(disconnect_err) = client.disconnect().await {
        // Log or handle disconnect error if needed
        return HttpResponse::InternalServerError().json(format!("Failed to disconnect: {}", disconnect_err));
    }
    HttpResponse::Ok().json(info)
}

async fn get_kaspad_info() -> impl Responder {
    let mut client = match get_client().await {
        Ok(client) => client,
        Err(err) => return err,
    };

   let info = match client.get_info().await {
        Ok(info) => info,
     Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get block DAG info: {:?}", err));
        }
    };
    if let Err(disconnect_err) = client.disconnect().await {
        // Log or handle disconnect error if needed
        return HttpResponse::InternalServerError().json(format!("Failed to disconnect: {}", disconnect_err));
    }
    HttpResponse::Ok().json(info)
}

async fn get_max_hashrate() -> impl Responder {
    // Attempt to get the client
    let client = match get_client().await {
        Ok(client) => client,
        Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get client: {:?}", err));
        }
    };

    // Attempt to get the block DAG info
    let block_dag_info = match client.get_block_dag_info().await {
        Ok(info) => info,
        Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get block DAG info: {:?}", err));
        }
    };

    // Fetch the latest block hash
    let latest_block_hash = block_dag_info.tip_hashes[0].clone();

    // Attempt to get the block
    let block = match client.get_block(latest_block_hash.clone(), false).await {
        Ok(block) => block,
        Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get block: {:?}", err));
        }
    };

    // Estimate the network hashrate
   let hashrate = match client.estimate_network_hashes_per_second(6000, Some(block.header.hash)).await {
        Ok(hashrate) => hashrate,
     Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get block DAG info: {:?}", err));
        }
    };
    if let Err(disconnect_err) = client.disconnect().await {
        // Log or handle disconnect error if needed
        return HttpResponse::InternalServerError().json(format!("Failed to disconnect: {}", disconnect_err));
    }
    HttpResponse::Ok().json(hashrate)
}

async fn get_coin_supply() -> impl Responder {
    let mut client = match get_client().await {
        Ok(client) => client,
        Err(err) => return err,
    };

   let supply = match client.get_coin_supply().await {
        Ok(supply) => supply ,
     Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get block DAG info: {:?}", err));
        }
    };
    if let Err(disconnect_err) = client.disconnect().await {
        // Log or handle disconnect error if needed
        return HttpResponse::InternalServerError().json(format!("Failed to disconnect: {}", disconnect_err));
    }
    HttpResponse::Ok().json(supply.circulating_sompi.to_string())

}

async fn get_balance_by_address(path: web::Path<String>) -> impl Responder {
    let addr = path.into_inner();
    let mut client = match get_client().await {
        Ok(client) => client,
        Err(err) => return err,
    };

    let balance =  match client.get_balance_by_address(RpcAddress::try_from(addr).unwrap()).await {
        Ok(balance) => balance,
      Err(err) => {
            return HttpResponse::InternalServerError().json(format!("Failed to get block DAG info: {:?}", err));
        }
    };
    if let Err(disconnect_err) = client.disconnect().await {
        // Log or handle disconnect error if needed
        return HttpResponse::InternalServerError().json(format!("Failed to disconnect: {}", disconnect_err));
    }
    HttpResponse::Ok().json(BalanceResponse { balance: balance.to_string() })
}
#[derive(Serialize)]
struct HalvingInfo {
    next_halving_timestamp: i64, // Timestamp in seconds
    next_halving_amount: f64,    // Halving amount
}

// Constants for your subsidy calculations
const PRE_DEFLATIONARY_PHASE_BASE_SUBSIDY: u64 = 50000000000; // Initial base subsidy in smallest unit
const DEFLATIONARY_PHASE_INITIAL_SUBSIDY: u64 = 44000000000; // Initial deflationary phase subsidy
const SECONDS_PER_MONTH: u64 = 2629800; // Number of seconds in a month
const SECONDS_PER_HALVING: u64 = SECONDS_PER_MONTH * 12; // Halving every 12 months

// Structure to hold the halving information


// Function to get blocks per second (example value)
fn get_blocks_per_second() -> u64 {
    40 // Assuming 40 blocks per second as given
}
#[derive(Serialize)]
struct HalvingResponse {
    next_halving_timestamp: i64,
    next_halving_date: String,
    next_halving_amount: f64,
}


// Calculate halving information
async fn calculate_halving_info(daa_score: u64) -> HalvingResponse {
    let mut data: HashMap<u64, f64> = HashMap::new();
    data.insert(15519600, 500.0);
    data.insert(18149400, 440.0);
    data.insert(20779200, 415.30469757);
    data.insert(23409000, 391.99543598);
    data.insert(26038800, 369.99442271);
    data.insert(28668600, 349.22823143);
    data.insert(31298400, 329.62755691);
    data.insert(33928200, 311.12698372);
    data.insert(36558000, 293.66476791);
    data.insert(39187800, 277.18263097);
    data.insert(41817600, 261.6255653);
    data.insert(44447400, 246.94165062);
    data.insert(47077200, 233.08188075);
    data.insert(49707000, 220.0);
    data.insert(52336800, 207.65234878);
    data.insert(54966600, 195.99771799);
    data.insert(57596400, 184.99721135);
    data.insert(60226200, 174.61411571);
    data.insert(62856000, 164.81377845);
    data.insert(65485800, 155.56349186);
    data.insert(68115600, 146.83238395);
    data.insert(70745400, 138.59131548);
    data.insert(73375200, 130.81278265);
    data.insert(76005000, 123.47082531);
    data.insert(78634800, 116.54094037);
    data.insert(81264600, 110.0);
    data.insert(83894400, 103.82617439);
    data.insert(86524200, 97.99885899);
    data.insert(89154000, 92.49860567);
    data.insert(91783800, 87.30705785);
    data.insert(94413600, 82.40688922);
    data.insert(97043400, 77.78174593);
    data.insert(99673200, 73.41619197);
    data.insert(102303000, 69.29565774);
    data.insert(104932800, 65.40639132);
    data.insert(107562600, 61.73541265);
    data.insert(110192400, 58.27047018);
    data.insert(112822200, 55.0);
    data.insert(115452000, 51.91308719);
    data.insert(118081800, 48.99942949);
    data.insert(120711600, 46.24930283);
    data.insert(123341400, 43.65352892);
    data.insert(125971200, 41.20344461);
    data.insert(128601000, 38.89087296);
    data.insert(131230800, 36.70809598);
    data.insert(133860600, 34.64782887);
    data.insert(136490400, 32.70319566);
    data.insert(139120200, 30.86770632);
    data.insert(141750000, 29.13523509);
    data.insert(144379800, 27.5);
    data.insert(147009600, 25.95654359);
    data.insert(149639400, 24.49971474);
    data.insert(152269200, 23.12465141);
    data.insert(154899000, 21.82676446);
    data.insert(157528800, 20.6017223);
    data.insert(160158600, 19.44543648);
    data.insert(162788400, 18.35404799);
    data.insert(165418200, 17.32391443);
    data.insert(168048000, 16.35159783);
    data.insert(170677800, 15.43385316);
    data.insert(173307600, 14.56761754);
    data.insert(175937400, 13.75);
    data.insert(178567200, 12.97827179);
    data.insert(181197000, 12.24985737);
    data.insert(183826800, 11.5623257);
    data.insert(186456600, 10.91338223);
    data.insert(189086400, 10.30086115);
    data.insert(191716200, 9.72271824);
    data.insert(194346000, 9.17702399);
    data.insert(196975800, 8.66195721);
    data.insert(199605600, 8.17579891);
    data.insert(202235400, 7.71692658);
    data.insert(204865200, 7.28380877);
    data.insert(207495000, 6.875);
    data.insert(210124800, 6.48913589);
    data.insert(212754600, 6.12492868);
    data.insert(215384400, 5.78116285);
    data.insert(218014200, 5.45669111);
    data.insert(220644000, 5.15043057);
    data.insert(223273800, 4.86135912);
    data.insert(225903600, 4.58851199);
    data.insert(228533400, 4.3309786);
    data.insert(231163200, 4.08789945);
    data.insert(233793000, 3.85846329);
    data.insert(236422800, 3.64190438);
    data.insert(239052600, 3.4375);
    data.insert(241682400, 3.24456794);
    data.insert(244312200, 3.06246434);
    data.insert(246942000, 2.89058142);
    data.insert(249571800, 2.72834555);
    data.insert(252201600, 2.57521528);
    data.insert(254831400, 2.43067956);
    data.insert(257461200, 2.29425599);
    data.insert(260091000, 2.1654893);
    data.insert(262720800, 2.04394972);
    data.insert(265350600, 1.92923164);
    data.insert(267980400, 1.82095219);
    data.insert(270610200, 1.71875);
    data.insert(273240000, 1.62228397);
    data.insert(275869800, 1.53123217);
    data.insert(278499600, 1.44529071);
    data.insert(281129400, 1.36417277);
    data.insert(283759200, 1.28760764);
    data.insert(286389000, 1.21533978);
    data.insert(289018800, 1.14712799);
    data.insert(291648600, 1.08274465);
    data.insert(294278400, 1.02197486);
    data.insert(296908200, 0.96461582);
    data.insert(299538000, 0.91047609);
    data.insert(302167800, 0.859375);
    data.insert(304797600, 0.81114198);
    data.insert(307427400, 0.76561608);
    data.insert(310057200, 0.72264535);
    data.insert(312687000, 0.68208638);
    data.insert(315316800, 0.64380382);
    data.insert(317946600, 0.60766989);
    data.insert(320576400, 0.57356399);
    data.insert(323206200, 0.54137232);
    data.insert(325836000, 0.51098743);
    data.insert(328465800, 0.48230791);
    data.insert(331095600, 0.45523804);
    data.insert(333725400, 0.4296875);
    data.insert(336355200, 0.40557099);
    data.insert(338985000, 0.38280804);
    data.insert(341614800, 0.36132267);
    data.insert(344244600, 0.34104319);
    data.insert(346874400, 0.32190191);
    data.insert(349504200, 0.30383494);
    data.insert(352134000, 0.28678199);
    data.insert(354763800, 0.27068616);
    data.insert(357393600, 0.25549371);
    data.insert(360023400, 0.24115395);
    data.insert(362653200, 0.22762313);
    data.insert(365283000, 0.21476591);
    data.insert(367912800, 0.20265196);
    data.insert(370542600, 0.19123776);
    data.insert(373172400, 0.18049956);
    data.insert(375802200, 0.17043676);
    data.insert(378432000, 0.1610347);
    data.insert(381061800, 0.15229382);
    data.insert(383691600, 0.14422656);
    data.insert(386321400, 0.13678283);
    data.insert(388951200, 0.12994654);
    data.insert(391581000, 0.12370799);
    data.insert(394210800, 0.11804224);
    data.insert(396840600, 0.11296396);
    data.insert(399470400, 0.10847332);
    data.insert(402100200, 0.10458033);
    data.insert(404730000, 0.10131789);
    data.insert(407359800, 0.09862433);
    data.insert(409989600, 0.09657248);
    data.insert(412619400, 0.09508814);
    data.insert(415249200, 0.09417745);
    data.insert(417879000, 0.09383286);
    data.insert(420508800, 0.09409615);
    data.insert(423138600, 0.09493826);
    data.insert(425768400, 0.09631169);
    data.insert(428398200, 0.09815882);
    data.insert(431028000, 0.1004122);
    data.insert(433657800, 0.10305375);
    data.insert(436287600, 0.10602993);
    data.insert(438917400, 0.10933378);
    data.insert(441547200, 0.11302845);
    data.insert(444177000, 0.11707016);
    data.insert(446806800, 0.12144084);
    data.insert(449436600, 0.12613759);
    data.insert(452066400, 0.13112062);
    data.insert(454696200, 0.13643943);
    data.insert(457326000, 0.14209691);
    data.insert(459955800, 0.1480862);
    data.insert(462585600, 0.15443784);
    data.insert(465215400, 0.16114343);
    data.insert(467845200, 0.16818979);
    data.insert(470475000, 0.17564051);
    data.insert(473104800, 0.18346341);
    data.insert(475734600, 0.1917266);
    data.insert(478364400, 0.20051084);
    data.insert(480994200, 0.20983509);
    data.insert(483624000, 0.21971124);
    data.insert(486253800, 0.2301438);
    data.insert(488883600, 0.24113258);
    data.insert(491513400, 0.25268619);
    data.insert(494143200, 0.26479714);
    data.insert(496773000, 0.27746917);
    data.insert(499402800, 0.29071021);
    data.insert(502032600, 0.30452164);
    data.insert(504662400, 0.31889863);
    data.insert(507292200, 0.33384339);
    data.insert(509922000, 0.34935715);
    data.insert(512551800, 0.36544036);
    data.insert(515181600, 0.38209539);
    data.insert(517811400, 0.39932595);
    data.insert(520441200, 0.41712519);
    data.insert(523071000, 0.43550739);
    data.insert(525700800, 0.45447742);
    data.insert(528330600, 0.47402891);
    data.insert(530960400, 0.49415404);
    data.insert(533590200, 0.51483542);
    data.insert(536220000, 0.53607358);
    data.insert(538849800, 0.5578492);
    data.insert(541479600, 0.58013131);
    data.insert(544109400, 0.60290554);
    data.insert(546739200, 0.62616163);
    data.insert(549369000, 0.64988689);
    data.insert(552998800, 0.67406554);
    data.insert(555628600, 0.69868107);
    data.insert(558258400, 0.72370715);
    data.insert(560888200, 0.74914643);
    data.insert(563518000, 0.77504293);
    data.insert(566147800, 0.80140805);
    data.insert(568777600, 0.82824862);
    data.insert(571407400, 0.85558615);
    data.insert(574037200, 0.88342548);
    data.insert(576667000, 0.91176738);
    data.insert(579296800, 0.94061089);
    data.insert(581926600, 0.96995827);
    data.insert(584556400, 0.99981739);
    data.insert(587186200, 1.03018801);
    data.insert(589816000, 1.06108733);
    data.insert(592445800, 1.09250556);
    data.insert(595075600, 1.12446462);
    data.insert(597705400, 1.15704183);
    data.insert(600335200, 1.19029283);
    data.insert(602965000, 1.22431324);
    data.insert(605594800, 1.25908533);
    data.insert(608224600, 1.29464635);
    data.insert(610854400, 1.33104201);
    data.insert(613484200, 1.36830438);
    data.insert(616114000, 1.40644768);
    data.insert(618743800, 1.44545977);
    data.insert(621373600, 1.48542605);
    data.insert(624003400, 1.52629285);
    data.insert(626633200, 1.56805948);
    data.insert(629263000, 1.61073863);
    data.insert(631892800, 1.65433896);
    data.insert(634522600, 1.69885337);
    data.insert(637152400, 1.74430241);
    data.insert(639782200, 1.7906807);
    data.insert(642412000, 1.83802013);
    data.insert(645041800, 1.88630739);
    data.insert(647671600, 1.93554642);
    data.insert(650301400, 1.98574777);
    data.insert(652931200, 2.03690322);
    data.insert(655561000, 2.08902358);
    data.insert(658190800, 2.14211662);
    data.insert(660820600, 2.19606986);
    data.insert(663450400, 2.25097958);
    data.insert(666080200, 2.30696614);
    data.insert(668710000, 2.36403037);
    data.insert(671339800, 2.4222131);
    data.insert(673969600, 2.48157869);
    data.insert(676599400, 2.54203909);
    data.insert(679229200, 2.60360241);
    data.insert(681859000, 2.66619334);
    data.insert(684488800, 2.72984534);
    data.insert(687118600, 2.79456452);
    data.insert(689748400, 2.86038066);
    data.insert(692378200, 2.92734309);
    data.insert(695008000, 2.99543411);
    data.insert(697637800, 3.06465069);
    data.insert(700267600, 3.13497407);
    data.insert(702897400, 3.2064109);
    data.insert(705527200, 3.27896566);
    data.insert(708157000, 3.35261108);

    let mut future_reward = 0.0;
    let mut daa_breakpoint = 0;

    // Create a sorted list of DAA breakpoints
    let mut daa_list: Vec<u64> = data.keys().cloned().collect(); // Collect keys and clone them
    daa_list.sort();

    // Find the next breakpoint and future reward
    for (i, &to_break_score) in daa_list.iter().enumerate() {
        if daa_score < to_break_score {
            // Ensure we don't go out of bounds
            if i + 1 < daa_list.len() {
                let next_break_score = daa_list[i + 1];
                if let Some(&reward_amount) = data.get(&next_break_score) {
                    future_reward = reward_amount;
                    daa_breakpoint = to_break_score;
                }
            }
            break;
        }
    }

    // Calculate the next halving timestamp
    let next_halving_timestamp = Utc::now().timestamp() + (daa_breakpoint as i64 - daa_score as i64);
    let next_halving_date = Utc.timestamp(next_halving_timestamp, 0).to_string(); // Format date

    HalvingResponse {
        next_halving_timestamp,
        next_halving_date,
        next_halving_amount: future_reward,
    }

}
// Function to calculate the next halving timestamp and amount
async fn get_halving() -> impl Responder {

    let mut client = match get_client().await {
        Ok(client) => client,
        Err(err) => return err,
    };
    // Step 2: Get the latest block's hash from the DAG info
    let block_dag_info = match client.get_block_dag_info().await {
        Ok(info) => info,
        Err(_) => return HttpResponse::InternalServerError().json("Failed to fetch block DAG info"),
    };
    let latest_block_hash = block_dag_info.tip_hashes[0].clone();

    // Step 3: Get the latest block using its hash, including transactions
    let block_result = match client.get_block(latest_block_hash, true).await {
        Ok(block) => block,
        Err(_) => return HttpResponse::InternalServerError().json("Failed to fetch block information"),
    };
    let halving_info = calculate_halving_info(block_result.header.daa_score).await;
    if let Err(disconnect_err) = client.disconnect().await {
        // Log or handle disconnect error if needed
        return HttpResponse::InternalServerError().json(format!("Failed to disconnect: {}", disconnect_err));
    }
    HttpResponse::Ok().json(halving_info)

}
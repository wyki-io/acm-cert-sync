extern crate env_logger;
extern crate rusoto_core;
extern crate rusoto_elbv2;
#[macro_use]
extern crate log;
use cert_sync::{Destination, Source, TLS};

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{Api, DeleteParams, ListParams, Meta, PostParams},
    Client,
};

pub struct TestSource {}

#[async_trait]
impl Source for TestSource {
    async fn receive<'a, T: Destination + Send + Sync>(
        &'a self,
        _destination: &'a T,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn name(&self) -> String {
        String::from("TestSource")
    }
}

pub struct TestDestination {
    pub tls: Arc<RwLock<TLS>>,
}

#[async_trait]
impl Destination for TestDestination {
    async fn publish(&self, tls: TLS) -> anyhow::Result<()> {
        let mut tls_write = self.tls.write().unwrap();
        *tls_write = tls;
        Ok(())
    }

    fn name(&self) -> String {
        String::from("Test")
    }
}

/// Creates a Kubernetes client
async fn init_client() -> Client {
    Client::try_default()
        .await
        .expect("Unabled to create client")
}

/// Deletes all certificates from Kubernetes
async fn delete_certificates(client: &Client) {
    let secrets: Api<Secret> = Api::all(client.clone());
    let lp = ListParams::default();
    let secret_list = secrets.list(&lp).await.expect("Unable to list secrets");

    for secret in secret_list {
        if secret.clone().type_.unwrap_or_default() == "kubernetes.io/tls" {
            let namespace = Meta::namespace(&secret).unwrap_or(String::from("default"));
            println!(
                "Deleting Secret with name {} in namespace {}",
                Meta::name(&secret),
                &namespace
            );
            let secrets_namespaced: Api<Secret> = Api::namespaced(client.clone(), &namespace);
            secrets_namespaced
                .delete("test-tls", &DeleteParams::default())
                .await
                .expect("Unable to delete secret");
        }
    }
}

/// Send given secret to Kubernetes
/// It expects a certificate secret, but do not check it
async fn add_certificate(client: &Client, file: &str) {
    // let path = Path::new(file);
    // dbg!(path);
    // let file = std::fs::File::open(&path).expect("Unable to open certificate file");
    let tls_secret: Secret = serde_yaml::from_str(file).expect("Unable to read file as YAML");
    // serde_yaml::from_reader(file).expect("Unable to convert certificate file to yaml");
    let secrets: Api<Secret> = Api::namespaced(client.clone(), "default");
    let post_params = PostParams::default();
    match secrets.create(&post_params, &tls_secret).await {
        Ok(res) => {
            let name = Meta::name(&res);
            assert_eq!(Meta::name(&tls_secret), name);
            info!("Created {}", name);
            // wait for it..
            // std::thread::sleep(std::time::Duration::from_millis(1_000));
        }
        Err(kube::Error::Api(ae)) => {
            dbg!(ae);
            ()
        } // if you skipped delete, for instance
        Err(e) => {
            dbg!("something bad happened {}", e);
            ()
        }
    }
}

/// Extract data value out of a Kubernetes Secret
fn get_data(secret: &Secret, key: &str) -> String {
    let value = secret.data.as_ref().unwrap().get(key).unwrap();
    String::from_utf8(value.0.clone()).unwrap()
}

// #[tokio::test(threaded_scheduler)]
// Test commented out as it begin to be unmaintainable and not accurate
async fn create_certificate() {
    let tls = Arc::new(RwLock::new(TLS::new(
        String::new(),
        String::new(),
        Vec::new(),
        Vec::new(),
    )));
    // It currently tests with an existing Kubernetes cluster
    let source = TestSource {};
    let destination = TestDestination { tls: tls.clone() };

    // Pre test cleanup
    let client = init_client().await;
    delete_certificates(&client).await;

    let kube_secret_file_path = include_str!("./resources/certificate.yaml");
    add_certificate(&client, kube_secret_file_path).await;

    // Sleep so that the server have time to receive the secret and compute it
    // Pretty ugly but haven't find a better way of handling it
    std::thread::sleep(std::time::Duration::from_millis(500));

    let tls_read = tls.read().unwrap();
    let kube_tls_secret: Secret =
        serde_yaml::from_str(kube_secret_file_path).expect("Unable to read file as YAML");
    assert_eq!(get_data(&kube_tls_secret, "tls.crt"), tls_read.cert);
    assert_eq!(get_data(&kube_tls_secret, "tls.key"), tls_read.key);

    // Post test cleanup
    delete_certificates(&client).await;
}

// use rusoto_core::Region;
// use rusoto_elbv2::{DescribeLoadBalancersInput, Elb, ElbClient};

// #[tokio::test]
// async fn rusoto_test() {
//     let _ = env_logger::try_init();
//     let client = ElbClient::new(Region::EuWest3);
//     let request = DescribeLoadBalancersInput::default();

//     let result = client.describe_load_balancers(request).await.unwrap();
//     println!("{:#?}", result);
// }

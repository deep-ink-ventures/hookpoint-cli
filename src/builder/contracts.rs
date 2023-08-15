// Copyright (C) Deep Ink Ventures GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Contracts Builder
//!
//! This module provides utilities and functions to generate ink! smart contract code
//! and corresponding TOML configurations. The generated contracts serve as the interface
//! between the Substrate runtime and the ink! smart contracts, based on the provided
//! `hookpoints.json` configuration.
//!
//! The main functionalities include generating boilerplate ink! contracts, ink! trait
//! definitions, and their respective TOML configuration files. The module uses the `Definitions`
//! structure from the configuration to extract the necessary information.

use toml::de::Error;
use crate::config::models::{Definitions, PalletFunction};
use crate::utils::{camel_case_to_kebab, camel_to_snake, get_default_ink_type_for_test, get_default_for_ink_type, INK_PRIMITIVES, INK_TYPES};

/// Generates TOML dependencies for the ink! contract based on the provided contract definitions.
/// This considers both the default ink! dependencies as well as specialized dependencies based on the
/// contract's requirements.
///
/// # Arguments
/// * `definitions` - The contract definitions derived from the configuration.
/// * `include_prelude` - Boolean indicating whether to include the ink_prelude dependency.
///
/// # Returns
/// Returns a formatted string representing the dependencies in TOML format.
fn generate_dependencies_toml(definitions: &Definitions, include_prelude: bool) -> String {
    let ink_deps = &definitions.ink_dependencies;

    // Check if we need to include ink_primitives
    let primitives_str = if definitions.contains_type(INK_PRIMITIVES) {
        format!("\nink_primitives = {{ version = \"{}\", default-features = false }}", ink_deps.ink_version)
    } else {
        String::new()
    };

    // Decide if we include ink_prelude
    let prelude_str = if include_prelude {
        format!("\nink_prelude = {{ version = \"{}\", default-features = false }}", ink_deps.ink_primitives_version)
    } else {
        String::new()
    };

    format!(
        r#"[dependencies]
ink = {{ version = "{}", default-features = false }}{}{}
scale = {{ package = "parity-scale-codec", version = "{}", default-features = false, features = ["derive"] }}
scale-info = {{ version = "{}", default-features = false, features = ["derive"], optional = true }}
"#, ink_deps.ink_version, prelude_str, primitives_str, ink_deps.scale_version, ink_deps.scale_info_version
    )
}


/// Generates the TOML configuration for the main ink! contract.
///
/// # Arguments
/// * `definitions` - The contract definitions derived from the configuration.
/// * `include_tests` - Boolean indicating whether to include the e2e-tests feature.
///
/// # Returns
/// Returns a Result containing the formatted TOML configuration string or a toml::de::Error.
pub fn generate_contract_toml(definitions: &Definitions, include_tests: bool) -> Result<String, toml::de::Error> {
    let ink_deps = &definitions.ink_dependencies;
    let name_kebab = camel_case_to_kebab(&definitions.name);

    let dev_dependencies = match include_tests {
        true => format!(r#"
[dev-dependencies]
ink_e2e = "{}"
"#, ink_deps.ink_version),
        false => String::new()
    };
    let e2e_feature = match include_tests {
        true => String::from("e2e-tests = []\n"),
        false => String::new()
    };

    let toml_string = format!(
        r#"[package]
name = "{}-contract"
version = "0.1.0"
edition = "2021"
authors = ["add your name here"]

{}
{}-contract-trait = {{ package = "{}-contract-trait", default-features = false, path = "../{}-contract-trait" }}
{}
[lib]
path = "lib.rs"

[features]
default = ["std"]
std = [
    "ink/std",
    "ink_prelude/std",
    "scale/std",
    "scale-info/std",
]
ink-as-dependency = []
{}
[workspace]
"#, name_kebab, generate_dependencies_toml(&definitions, true), name_kebab, name_kebab, name_kebab, dev_dependencies, e2e_feature);

    // Validate the TOML
    let parsed: Result<toml::Value, Error> = toml::from_str(&toml_string);
    match parsed {
        Ok(_) => Ok(toml_string),
        Err(err) => Err(err)
    }
}

/// Generates the TOML configuration for the trait of the ink! contract.
///
/// # Arguments
/// * `definitions` - The contract definitions derived from the configuration.
///
/// # Returns
/// Returns a Result containing the formatted TOML configuration string or a toml::de::Error.
pub fn generate_contract_trait_toml(definitions: &Definitions) -> Result<String, Error> {
    let name_kebab = camel_case_to_kebab(&definitions.name);

    let toml_string = format!(
        r#"[package]
name = "{}-contract-trait"
version = "0.1.0"
edition = "2021"

{}
[lib]
path = "lib.rs"

[features]
default = ["std"]
std = [
    "ink/std",
    "scale/std",
    "scale-info/std",
]
ink-as-dependency = []

[workspace]
"#, name_kebab, generate_dependencies_toml(&definitions, false));

    // Validate the TOML
    let parsed: Result<toml::Value, Error> = toml::from_str(&toml_string);
    match parsed {
        Ok(_) => Ok(toml_string),
        Err(err) => Err(err)
    }
}

/// Generates the ink! trait signature based on the provided contract definitions.
///
/// # Arguments
/// * `definitions` - The contract definitions derived from the configuration.
///
/// # Returns
/// Returns a formatted string representing the ink! trait.
pub fn generate_ink_trait(definitions: &Definitions) -> String {
    let function_signatures: Vec<String> = definitions.pallets
        .iter()
        .flat_map(|(_, pallet_functions)| {
            pallet_functions
                .iter()
                .map(|function| generate_trait_function_signature(function))
                .collect::<Vec<_>>()
        })
        .collect();

    // Check for types
    let mut ink_primitives: Vec<&str> = Vec::new();
    for prim in INK_PRIMITIVES.iter() {
        if definitions.contains_type(&[prim]) {
            ink_primitives.push(prim);
        }
    }

    let mut import_string = match ink_primitives.len() {
        0 => String::new(),
        1 => format!("use ink_primitives::{};\n", ink_primitives[0]),
        _ => format!("use ink_primitives::{{{}}};\n", ink_primitives.join(", ")),
    };

    let mut vector_type: Vec<&str> = Vec::new();
    for prelude in INK_TYPES.iter() {
        if prelude.starts_with("Vec") && definitions.contains_type(&[prelude]) {
            vector_type.push(prelude);
        }
    }
    if !vector_type.is_empty() {
        import_string.push_str(&format!("\nuse ink::prelude::vec::Vec;\n"));
    }

    if definitions.contains_type(&["Balance"]) {
        import_string.push_str("\ntype Balance = <ink::env::DefaultEnvironment as ink::env::Environment>::Balance;\n");
    }

    format!(r##"#![cfg_attr(not(feature = "std"), no_std, no_main)]
{imports}
#[ink::trait_definition]
pub trait {trait_name} {{

{function_signatures}
}}"##,
            trait_name = definitions.name,
            function_signatures = function_signatures.join("\n\n"),
            imports = import_string
    )
}

/// Generates the signature for a given function to be used in an ink! trait.
///
/// # Arguments
/// * `func` - A reference to the pallet function for which the signature is to be generated.
///
/// # Returns
/// Returns a formatted string representing the trait function signature.
fn generate_trait_function_signature(func: &PalletFunction) -> String {
    let args = func.arguments
        .iter()
        .map(|arg| format!("{name}: {type_}", name = arg.name, type_ = arg.type_))
        .collect::<Vec<_>>()
        .join(", ");

    let method_args = if args.is_empty() {
        "&self".to_string()
    } else {
        format!("&self, {}", args)
    };

    let return_type = if let Some(ret_val) = &func.returns {
        format!(" -> {}", ret_val.type_)
    } else {
        String::new()
    };

    format!(r##"    /// hook point for `{hook_point}` pallet
    #[ink(message)]
    fn {hook_point}({method_args}){return_type};"##,
            hook_point = func.hook_point,
            method_args = method_args,
            return_type = return_type
    )
}

/// Generates the contract functions based on the provided definitions.
///
/// # Arguments
/// * `definitions` - The contract definitions derived from the configuration.
///
/// # Returns
/// Returns a formatted string with the generated contract functions.
fn generate_contract_functions(definitions: &Definitions) -> String {
    let functions: Vec<String> = definitions
        .pallets
        .iter()
        .flat_map(|(_, pallet_functions)| {
            pallet_functions
                .iter()
                .map(|function| generate_function_body(function))
                .collect::<Vec<_>>()
        })
        .collect();

    // Adjust the spaces for indentation to align with the `impl` block
    functions
        .iter()
        .map(|f| format!("        {}", f))
        .collect::<Vec<String>>()
        .join("\n\n")
}

/// Generates the body of a specific contract function.
///
/// # Arguments
/// * `func` - A reference to the pallet function for which the body is to be generated.
///
/// # Returns
/// Returns a formatted string representing the function body.
fn generate_function_body(func: &PalletFunction) -> String {
    let args = func
        .arguments
        .iter()
        .map(|arg| {
            if func.returns.is_some() && func.returns.as_ref().unwrap().default == arg.name {
                format!("{name}: {type_}", name = arg.name, type_ = arg.type_)
            } else {
                format!("_{name}: {type_}", name = arg.name, type_ = arg.type_)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let return_type = if let Some(ret_val) = &func.returns {
        if INK_TYPES.contains(&ret_val.default.as_str()) {
            format!(" -> {}", ret_val.type_)
        } else if func.arguments.iter().any(|arg| arg.name == ret_val.default) {
            format!(" -> {}", ret_val.type_)
        } else {
            format!(" -> {}", ret_val.type_)
        }
    } else {
        String::new()
    };

    let function_body = if let Some(ret_val) = &func.returns {
        if INK_TYPES.contains(&ret_val.default.as_str()) {
            get_default_for_ink_type(&ret_val.type_)
        } else {
            ret_val.default.clone()
        }
    } else {
        "// do nothing".to_string()
    };

    format!(
        r##"/// hook point for `{hook_point}` pallet
        #[ink(message)]
        fn {hook_point}(&self{args}){return_type} {{
            {function_body}
        }}"##,
        hook_point = func.hook_point,
        args = if args.is_empty() { String::new() } else { format!(", {}", args) },
        return_type = return_type,
        function_body = function_body
    )
}

/// Generates test functions for ink! based on the provided definitions.
///
/// # Arguments
/// * `definitions` - The contract definitions derived from the configuration.
///
/// # Returns
/// Returns a formatted string with the generated test functions.
fn generate_ink_test_functions(definitions: &Definitions) -> String {
    let tests: Vec<String> = definitions
        .pallets
        .iter()
        .flat_map(|(_, pallet_functions)| {
            pallet_functions
                .iter()
                .map(|function| generate_test_function(function, &definitions.name))
                .collect::<Vec<_>>()
        })
        .collect();

    tests.join("\n")
}

pub fn generate_e2e_test_functions(definitions: &Definitions) -> String {
    let mut e2e_tests = String::new();

    for (pallet_name, pallet_functions) in &definitions.pallets {
        for function in pallet_functions {
            let e2e_function_name = format!("e2e_test_{}", function.hook_point);
            let ref_type = format!("{}Ref", definitions.name);
            let snake_case_name = camel_to_snake(&definitions.name);

            let parameters = function.arguments.iter().map(|param| {
                get_default_ink_type_for_test(&param.type_)
            }).collect::<Vec<_>>().join(", ");

            let expected_return = if let Some(ret_val) = &function.returns {
                get_default_ink_type_for_test(&ret_val.type_)
            } else {
                "()".to_string()
            };

            let test_function = format!(r#"
        #[ink_e2e::test]
        async fn {e2e_function_name}(mut client: ink_e2e::Client<C, E>) -> E2EResult<()> {{
            let constructor = {ref_type}::new();

            let contract_account_id = client
                .instantiate("{snake_case_name}", &ink_e2e::alice(), constructor, 0, None)
                .await
                .expect("instantiate failed")
                .account_id;

            let {hook_name} = build_message::<{ref_type}>(contract_account_id.clone())
                .call(|{definition_name}| {definition_name}.{hook_name}({parameters}));

            let result = client
                .call_dry_run(&ink_e2e::alice(), &{hook_name}, 0, None)
                .await;

            if let Some(ret) = &function.returns {{
                assert_eq!(result.return_value(), {expected_return});
            }}
            Ok(())
        }}
"#, e2e_function_name = e2e_function_name, ref_type = ref_type, snake_case_name = snake_case_name, hook_name = function.hook_point, definition_name = pallet_name, parameters = parameters, expected_return = expected_return);

            e2e_tests.push_str(&test_function);
        }
    }

    let wrapped_e2e_tests = format!(r#"
    #[cfg(all(test, feature = "e2e-tests"))]
    mod e2e_tests {{
        use super::*;
        use {definition_name_lower}_contract_trait::{definition_name} as Trait;
        use ink_e2e::build_message;
        type E2EResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
        {e2e_tests}
    }}
"#, definition_name_lower = camel_to_snake(&definitions.name.as_str()), definition_name=definitions.name, e2e_tests = e2e_tests);

    wrapped_e2e_tests
}

/// Generates a single test function for a given contract function.
///
/// # Arguments
/// * `func` - A reference to the pallet function for which the test is to be generated.
/// * `contract_name` - The name of the contract for which the test is being generated.
///
/// # Returns
/// Returns a formatted string representing the test function.
fn generate_test_function(func: &PalletFunction, contract_name: &str) -> String {
    let contract_instance = format!(
        "let {} = {}::new();",
        camel_to_snake(contract_name),
        contract_name
    );

    let arguments: Vec<String> = func
        .arguments
        .iter()
        .map(|arg| get_default_for_ink_type(&arg.type_))
        .collect();

    let expected_return = if let Some(ret_val) = &func.returns {
        get_default_ink_type_for_test(&ret_val.type_)
    } else {
        "()".to_string()
    };

    format!(
        r##"
        #[ink::test]
        fn test_{hook_point}_hookpoint() {{
            {contract_instance}
            assert_eq!({contract_snake_name}.{hook_point}({arguments}), {expected_return});
        }}"##,
        hook_point = func.hook_point,
        contract_instance = contract_instance,
        contract_snake_name = camel_to_snake(contract_name),
        arguments = arguments.join(", "),
        expected_return = expected_return
    )
}

/// Generates the full boilerplate for an ink! contract based on provided definitions.
///
/// # Arguments
/// * `definitions` - The contract definitions derived from the configuration.
/// * `include_tests` - A boolean indicating whether to generate test boilerplate.
///
/// # Returns
/// Returns a formatted string representing the complete boilerplate for the ink! contract.
pub fn generate_ink_contract(definitions: &Definitions, include_tests: bool) -> String {
    let functions = generate_contract_functions(definitions);
    let contract_name = &definitions.name;
    let contract_name_lower = camel_to_snake(contract_name);

    // Conditionally generate the test boilerplate
    let test_boilerplate = if include_tests {
        let mut tests = format!(
            r##"
    #[cfg(test)]
    mod tests {{
        use super::*;
        use {contract_name_lower}_contract_trait::{contract_name} as Trait;
        {ink_test_functions}
    }}
"##,
            contract_name_lower = contract_name_lower,
            contract_name = contract_name,
            ink_test_functions = generate_ink_test_functions(&definitions)
        );
        tests.push_str(generate_e2e_test_functions(&definitions).as_str());
        tests
    } else {
        String::new()
    };

    format!(
        r##"#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod {contract_name_lower} {{
    #[ink(storage)]
    pub struct {contract_name} {{}}

    impl {contract_name} {{
        #[ink(constructor)]
        pub fn new() -> Self {{
            Self {{}}
        }}
    }}

    impl {contract_name_lower}_contract_trait::{contract_name} for {contract_name} {{
{functions}
    }}
{test_boilerplate}
}}"##,
        contract_name = contract_name,
        contract_name_lower = contract_name_lower,
        functions = functions,
        test_boilerplate = test_boilerplate
    )
}

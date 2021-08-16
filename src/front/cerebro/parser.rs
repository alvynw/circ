use serde_json::Value;
use rug::Integer;
use std::collections::BTreeMap;

use crate::ir::term::*;

use std::io::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use std::collections::HashMap;

pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<Value, Error> {
    // Open the file in read-only mode with buffer.
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    //assumes top level node is a Module node
    let module = serde_json::from_reader(reader)?;

    Ok(module)
}

//assumes that the starting Python AST node is a Module
//assumes unroll and inline flags have been set
pub fn convert(module: Value, parties: u8) -> Computation {

    //module is assumed to be a Module node in Python's ast standard library

    //assume two parties
    let mut metadata: ComputationMetadata = ComputationMetadata::default();
    for i in 0..parties {
        let party_name = format!("Party {}", i);
        metadata.add_party(party_name);
    }

    //symbol table for the computation
    let mut symbol_table: HashMap<String, Term> = HashMap::new();

    let mut outputs: Vec<Term> = Vec::new();

    //Modules have a `body` entry
    //process every node in `body`
    for value in module["body"].as_array().unwrap() {
        evaluate_stmt(value.clone(), &mut symbol_table, &mut metadata, &mut outputs);
    }

    return Computation { outputs: outputs, metadata: metadata, ..Default::default() };
}

//
fn evaluate_stmt(value: Value, symbol_table: &mut HashMap<String, Term>, metadata: &mut ComputationMetadata, outputs: &mut Vec<Term>) -> () {
    match value["_type"].as_str().unwrap().as_ref() {
        "Assign" => {

            //Compute LHS -- 3 cases: Name, Subscript, Subscript-Subscript

            let target = value["targets"][0].clone();
            let target_type = target["_type"].as_str().unwrap().to_string();

            let mut identifier = "shouldn't happen".to_owned();

            //normal assignment
            if target_type == "Name" {
                identifier = target["id"].as_str().unwrap().to_string();
            } else if target_type == "Subscript" {
                let value_type = target["value"]["_type"].as_str().unwrap().to_string();
                //indexing array
                if value_type == "Name" {
                    identifier = target["value"]["id"].as_str().unwrap().to_string();
                //indexing matrix
                } else {
                    identifier = target["value"]["value"]["id"].as_str().unwrap().to_string();
                }
            } 

            //compute RHS of assignment
            let val: Term = evaluate_expr(value["value"].clone(), symbol_table, metadata, &identifier);

            //normal assignment
            if target_type == "Name" {
                symbol_table.insert(identifier, val);
            } else if target_type == "Subscript" {
                let value_type = target["value"]["_type"].as_str().unwrap().to_string();
                //indexing array
                if value_type == "Name" {
                    let array = (*symbol_table.get(&identifier).unwrap()).clone();
                    let index = evaluate_expr(target["slice"]["value"].clone(), symbol_table, metadata, "");

                    let new_array = term![Op::Store; array, index, val];

                    symbol_table.insert(identifier, new_array);
                //indexing matrix
                } else {
                    let matrix = (*symbol_table.get(&identifier).unwrap()).clone();
                    let col = evaluate_expr(target["slice"]["value"].clone(), symbol_table, metadata, "");
                    let row = evaluate_expr(target["value"]["slice"]["value"].clone(), symbol_table, metadata, "");

                    let changed_array = term![Op::Store; term![Op::Select; matrix.clone(), row.clone()], col.clone(), val];

                    let new_val = term![Op::Store; matrix, row, changed_array];

                    symbol_table.insert(identifier, new_val);
                }
            } 

            //update symbol table
            
        },
        "AugAssign" => {
            //assume there's only 1 target and that the target is a Name
            let target: String = value["target"]["id"].as_str().unwrap().to_string();

            //`target` should already be in symbol table
            if !symbol_table.contains_key(&target) {
                println!("!!!!!!!!!!!!!")
            }

            let sort: Sort = check_rec(symbol_table.get(&target).unwrap());
            println!("Sort in AugAssign: {}", sort);
            
            //compute RHS of assignment
            let val: Term = evaluate_expr(value["value"].clone(), symbol_table, metadata, "");
            
            let op_str = value["op"]["_type"].as_str().unwrap().to_string();
            let op: Op = map_nary_op(sort, op_str);

            println!("Op in AugAssign: {}", op);

            //create new term with binop of old term and the RHS
            let new_term: Term = term![op; (*symbol_table.get(&target).unwrap()).clone(), val];
            println!("Sort in AugAssign of new term: {}", check_rec(&new_term));

            //add into symbol_table
            symbol_table.insert(target, new_term);
            
        },
        "Expr" => {
            //Assumes is reveal_all or test etc 
                //i.e void functions
            
            //Assume Expr contains Call
            let call = value["value"].clone();
            let func = call["func"].clone();
            let func_name = func["id"].clone().as_str().unwrap().to_string();

            if func_name == "reveal_all" {
                let reveal_var = call["args"][0]["id"].as_str().unwrap().to_string();
                let reveal_var_term = (*symbol_table.get(&reveal_var).unwrap()).clone();
                outputs.push(reveal_var_term);
            } else if func_name == "test" {
                let left = evaluate_expr(call["args"][0].clone(), symbol_table, metadata, "");
                let right = evaluate_expr(call["args"][1].clone(), symbol_table, metadata, "");

                let eq = term![Op::Eq; left, right];
                let ite = term![Op::Ite; eq, 
                            leaf_term(Op::Const(crate::ir::term::Value::Bool(true))), 
                            leaf_term(Op::Const(crate::ir::term::Value::Bool(false)))];

                outputs.push(ite);
            }
        },
        "FunctionDef" => {
            return;
        },
        _ => {
            //TODO: throw error
            //for now just print that it's unsupported
            println!("Type not supported: {:?}", value["_type"].as_str().unwrap());
        }
        
    }
}

fn evaluate_expr(value: Value, symbol_table: &mut HashMap<String, Term>, metadata: &mut ComputationMetadata, name: &str) -> Term {
    match value["_type"].as_str().unwrap().as_ref() {
        
        "BinOp" => {
            let left = evaluate_expr(value["left"].clone(), symbol_table, metadata, "");
            let sort_left: Sort = check_rec(&left);

            let right = evaluate_expr(value["right"].clone(), symbol_table, metadata, "");
            let sort_right: Sort = check_rec(&right);

            if sort_left != sort_right {
                //throw some error
            }

            let op_str = value["op"]["_type"].as_str().unwrap().to_string();

            //map op to one of the BvBinOps
            let op = map_nary_op(sort_left, op_str);

            return term![op; left, right];
        }, 
        "Call" => {

            let func = value["func"].clone();

            let func_type = func["_type"].as_str().unwrap().to_string();

            if func_type == "Attribute" {
                let func_name = func["value"]["id"].as_str().unwrap().to_string();

                if func_name == "s_int" {
                    let party_id = value["args"][0]["n"].as_u64().unwrap() as u8;
                                
                    metadata.new_input(name.to_string(), Some(party_id));

                    //32-bit BitVector
                    return leaf_term(Op::Var(name.to_string(), Sort::BitVector(32)));

                } else if func_name == "s_int_array" {
                    let length = value["args"][0]["n"].as_u64().unwrap() as usize;
                    let party_id = value["args"][1]["n"].as_u64().unwrap() as u8;

                    metadata.new_input(name.to_string(), Some(party_id));

                    //Array of 32-bit BitVectors
                    return leaf_term(Op::Var(name.to_string(), Sort::Array(Box::new(Sort::BitVector(32)), Box::new(Sort::BitVector(32)), length)));
                } else if func_name == "s_int_mat" {
                    let rows = value["args"][0]["n"].as_u64().unwrap() as usize;
                    let cols = value["args"][1]["n"].as_u64().unwrap() as usize;
                    let party_id = value["args"][2]["n"].as_u64().unwrap() as u8;

                    metadata.new_input(name.to_string(), Some(party_id));

                    //Array of Array of 32-bit BitVectors
                    return leaf_term(Op::Var(name.to_string(), 
                        Sort::Array(
                            Box::new(Sort::BitVector(32)), 
                            Box::new(Sort::Array(Box::new(Sort::BitVector(32)), Box::new(Sort::BitVector(32)), cols)),
                            rows)
                        )
                    );
                }

            } else if func_type == "Name" {
                let func_name = func["id"].as_str().unwrap().to_string();

                if func_name == "c_int" {
                    //assume it's c_int
                    let c_int_val = value["args"][0]["n"].as_i64().unwrap() as u32;
                    let uint_integer = Integer::from(c_int_val);
                    let a: usize = 32;
                    let bv: BitVector = BitVector::new(uint_integer, a);

                    return leaf_term(Op::Const(crate::ir::term::Value::BitVector(bv)));
                } else if func_name == "c_int_array" {
                    let length = value["args"][0]["n"].as_i64().unwrap() as usize;

                    let zero_bv = BitVector::new(Integer::from(0), 32); 

                    return leaf_term(Op::Const(
                        crate::ir::term::Value::Array(
                            Sort::BitVector(32),
                            Box::new(crate::ir::term::Value::BitVector(zero_bv)),
                            BTreeMap::new(),
                            length
                        )
                    ));                    

                } else if func_name == "c_int_mat" {
                    let rows = value["args"][0]["n"].as_u64().unwrap() as usize;
                    let cols = value["args"][1]["n"].as_u64().unwrap() as usize;
                    
                    //Array of Array of 32-bit BitVectors
                    let zero_bv = BitVector::new(Integer::from(0), 32);

                    let zero_array = crate::ir::term::Value::Array(
                        Sort::BitVector(32),
                        Box::new(crate::ir::term::Value::BitVector(zero_bv)),
                        BTreeMap::new(),
                        cols
                    );

                    return leaf_term(Op::Const(
                        crate::ir::term::Value::Array(
                            Sort::BitVector(32),
                            Box::new(zero_array),
                            BTreeMap::new(),
                            rows
                        )
                    ));       

                } else if func_name == "array_index_secret_load_if" {
                    //res = array_index_secret_load_if(cond, array, index1, index2)
                        //does res = cond ? array[index1] : array[index2] 

                    let cond = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, name);
                    let array = evaluate_expr(value["args"][1].clone(), symbol_table, metadata, name);
                    let index1 = evaluate_expr(value["args"][2].clone(), symbol_table, metadata, name);
                    let index2 = evaluate_expr(value["args"][3].clone(), symbol_table, metadata, name);

                    let array_index1 = term![Op::Select; array.clone(), index1];
                    let array_index2 = term![Op::Select; array.clone(), index2];

                    return term![Op::Ite; cond, array_index1, array_index2];
                } else if func_name == "matadd" {
                    let a = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");
                    let b = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");

                    return term![Op::MatrixBinOp(MatrixBinOp::Add); a, b];
                } else if func_name == "matsub" {
                    let a = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");
                    let b = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");

                    return term![Op::MatrixBinOp(MatrixBinOp::Sub); a, b];
                    
                } else if func_name == "matmul" {
                    let a = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");
                    let b = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");

                    return term![Op::MatrixBinOp(MatrixBinOp::Mul); a, b];
                } else if func_name == "transpose" {
                    let a = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");

                    return term![Op::MatrixUnOp(MatrixUnOp::Transpose); a];
                } else if func_name == "inverse" {
                    let a = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");

                    return term![Op::MatrixUnOp(MatrixUnOp::Inverse); a];
                } else if func_name == "get_identity_matrix" {
                    let size = value["args"][0]["n"].as_i64().unwrap() as usize;

                    let zero_bv = BitVector::new(Integer::from(0), 32);
                    let one_bv = BitVector::new(Integer::from(1), 32);

                    let mut array_maps = vec![BTreeMap::new(); size];

                    for (i, map) in array_maps.iter_mut().enumerate() {
                        map.insert(
                            crate::ir::term::Value::BitVector(BitVector::new(Integer::from(i), 32)), 
                            crate::ir::term::Value::BitVector(one_bv.clone())
                        );
                    }

                    let mut arrays = Vec::new();

                    for i in 0..size {
                        arrays.push(crate::ir::term::Value::Array(
                            Sort::BitVector(32),
                            Box::new(crate::ir::term::Value::BitVector(zero_bv.clone())),
                            array_maps[i].clone(),
                            size
                        ));
                    }

                    let mut matrix_map = BTreeMap::new();

                    for (i, array) in arrays.iter().enumerate() {
                        matrix_map.insert(
                            crate::ir::term::Value::BitVector(BitVector::new(Integer::from(i), 32)), 
                            array.clone()
                        );
                    }

                    let zero_array = crate::ir::term::Value::Array(
                        Sort::BitVector(32),
                        Box::new(crate::ir::term::Value::BitVector(zero_bv)),
                        BTreeMap::new(),
                        size
                    );

                    return leaf_term(Op::Const(
                        crate::ir::term::Value::Array(
                            Sort::BitVector(32),
                            Box::new(zero_array),
                            matrix_map,
                            size
                        )
                    ));

                } else if func_name == "mat_assign" {

                    //assume src and dest are Names
                    let dest = value["args"][0]["id"].as_str().unwrap().to_string();
                    let src = value["args"][1]["id"].as_str().unwrap().to_string();

                    let src_clone = (*symbol_table.get(&src).unwrap()).clone();

                    symbol_table.insert(dest, src_clone);
                } else if func_name == "mat_const_mul" {
                    //should be Num
                    let scale = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");

                    //should be Name
                    let mat_key = value["args"][1]["id"].as_str().unwrap().to_string();

                    let mat = (*symbol_table.get(&mat_key).unwrap()).clone();

                    return term![Op::MatrixBinOp(MatrixBinOp::Scale); scale, mat];

                    
                } else if func_name == "sigmoid" {
                    let v = evaluate_expr(value["args"][0].clone(), symbol_table, metadata, "");
                    return term![Op::Sigmoid; v];
                }
                
            }

            return leaf_term(Op::Const(crate::ir::term::Value::F32(1.2345)));
            
        },
        "Compare" => {
            let left = evaluate_expr(value["left"].clone(), symbol_table, metadata, "");
            let sort_left: Sort = check_rec(&left);

            //assume only one (other) comparator
            let right = evaluate_expr(value["comparators"][0].clone(), symbol_table, metadata, "");
            let sort_right: Sort = check_rec(&right);

            if sort_left != sort_right {
                //throw error
            }

            //assume only comparison operation
            let cmpop = value["ops"][0]["_type"].as_str().unwrap().to_string();
            let op = map_nary_op(sort_left, cmpop);
            
            return term![op; left, right];
        },
        "Name" => {
            let id = value["id"].as_str().unwrap().to_string();
            return (*symbol_table.get(&id).unwrap()).clone();
        },
        "Num" => {
            //assume >= 0
            //assume bit vector length is 32 bits   
            let uint = value["n"].as_i64().unwrap() as u32;
            let uint_integer = Integer::from(uint);
            let a: usize = 32;
            let bv: BitVector = BitVector::new(uint_integer, a);
            return leaf_term(Op::Const(crate::ir::term::Value::BitVector(bv)));
        },
        _ => {
            //TODO: Throw Error
            //For now, return a Term containing a const of pi
            return leaf_term(Op::Const(crate::ir::term::Value::F32(5.678)));
        }
    }
}

fn map_nary_op(sort: Sort, op_name: String) -> Op {
    match sort {
        Sort::BitVector(_) => {
            match op_name.as_ref() {
                "Add" => {
                    return BV_ADD;
                },
                "Sub" => {
                    return BV_SUB;
                },
                "Mult" => {
                    return BV_MUL;
                },
                _ => {
                    return BV_AND;
                }
            }
        },
        _ => {
            return AND;
        }
    }
}

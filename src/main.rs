use crate::similarity::cosine_similarity;

mod similarity;

fn main() {
    let v1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let v2 = vec![-1.0, -2.0, -3.0, -4.0, -5.0];

    let res = cosine_similarity(&v1, &v2);

    println!("{res}");
}

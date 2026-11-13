(*  ShIR_TextOps_Device — the device boundary of the slice: the `GpuOp` node (a
    shell write lifted to a GPU dispatch, docs/BASH-O4.md §4.2 stage ②) and the
    `CgoCall` node (a call into a CGO-bound native library).

    Both nodes were recorded as unstated for a named missing model: device memory
    and dispatch for one, foreign-call marshalling for the other. The lesson of
    the `ForEachLine` section is that a named missing model is not the same thing
    as nothing to prove: the part of each node's contract that is a property of
    the NODE (rather than of the device or the foreign function) is a function of
    the node's own fields, and can be settled here. This file settles exactly that
    part and says, in each node's body and at the end of each section, what is
    left to the hardware or the callee.

    GpuOp:  the DEVICE PATH (the verb is the last segment, the device root plus
            the slot is everything before it) and the LAUNCH GEOMETRY (ceil
            groups, contiguous block ids, a launch that tiles the grid exactly —
            every work item dispatched, none twice).

    CgoCall: the ARGUMENT LAYOUT (positional, in declaration order, each argument
            exactly once at its own offset, exact total size, no padding).

    Proof style follows docs/fm/README.md: structural induction, explicit cases, no
    `metis`, no `simp` on a shape whose hypothesis would have to be re-derived, and
    `by eval` for every ground statement (ground statements avoid `set`, which is
    not a code equation — membership is stated as a filter count instead).

    Build:  isabelle build -D sh2perl/docs/fm/TextOps
*)

theory ShIR_TextOps_Device
  imports ShIR_TextOps_Base
begin


section \<open>GpuOp: the path algebra of a device write\<close>

text \<open>A `GpuOp` is a shell write to a device file, lifted out of the program and into
  the device boundary:

    echo "$blk_p" > /dev/fb0/blocks
    echo "swap"   > /dev/fb0/call

  Its fields are the device root (`/dev/fb0`), the `slot` (the sub-segments before the
  verb, joined with `/`, empty for a top-level verb), the `verb` (the LAST path segment,
  i.e. the operation) and the payload. Whatever a backend does with the dispatch, the
  path it denotes is the device root, then each segment of the slot followed by a slash,
  then the verb. `path_join` builds exactly that, keeping the verb as the TAIL rather
  than as one more segment: that is the node's claim that the last path segment is the
  verb, and it makes the construction structural in the segment list, so the laws below
  are one-line unfoldings rather than an induction that has to split a singleton.\<close>

definition path_join :: "bytes list \<Rightarrow> bytes \<Rightarrow> bytes" where
  "path_join segs verb \<equiv> concat (map (\<lambda>s. s @ [47]) segs) @ verb"

lemma path_join_nil [simp]: "path_join [] verb = verb"
  unfolding path_join_def by simp

lemma path_join_cons [simp]:
  "path_join (s # segs) verb = s @ [47] @ path_join segs verb"
  unfolding path_join_def by simp

text \<open>The node's own law, device first: the written path is the device root, a slash,
  then the slot's segments and the verb. The empty slot — the top-level verb
  `echo swap > /dev/fb0/call` — is the same statement at `slot = []`.\<close>

theorem gpu_path_last_segment:
  "path_join (device # slot) verb = device @ [47] @ path_join slot verb"
  by (simp add: path_join_cons)

text \<open>And the verb is TEXT at the tail: appending to the verb appends to the path, so
  the device reads exactly the bytes the node was handed, unmangled.\<close>

theorem gpu_path_verb_is_the_tail:
  "path_join segs (verb @ w) = path_join segs verb @ w"
  unfolding path_join_def by simp

text \<open>Ground witnesses, byte for byte: `/dev` + `/` + `fb0` + `/` + `call`, and the
  empty-slot form `/dev/fb0` + `/` + `swap`.\<close>

lemma gpu_device_path_witness:
  "path_join [[47, 100, 101, 118], [102, 98, 48]] [99, 97, 108, 108]
     = [47, 100, 101, 118, 47, 102, 98, 48, 47, 99, 97, 108, 108]"
  by eval

lemma gpu_path_empty_slot_witness:
  "path_join [[47, 100, 101, 118, 47, 102, 98, 48]] [115, 119, 97, 112]
     = [47, 100, 101, 118, 47, 102, 98, 48, 47, 115, 119, 97, 112]"
  by eval


section \<open>GpuOp: the launch geometry\<close>

text \<open>A dispatch of `n` work items on blocks of `b` threads is three functions: the
  number of groups (ceil, not floor: a partial last group still runs), the global thread
  id of a `(block, thread)` pair, and the launch list a backend emits — block 0's
  threads, then block 1's, and so on. `groups` is the only arithmetic here, and the two
  facts that matter about it are that it COVERS the work and that it does not add a
  gratuitous group.

  The tiling law below is what makes "every work item dispatched exactly once" a theorem.
  The ids of block `g` are `g * b + t` for `t < b`, so every id of an earlier block is
  smaller than every id of a later one (`block_ids_lt`) — distinct blocks are disjoint
  with no modular arithmetic at all — and a launch of `G` blocks of `b` threads is then a
  duplicate-free list of exactly `G * b` ids, every one of them below `G * b`.\<close>

definition groups :: "nat \<Rightarrow> nat \<Rightarrow> nat" where
  "groups n b \<equiv> (n + b - 1) div b"

definition gid :: "nat \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> nat" where
  "gid b g t \<equiv> g * b + t"

definition block_ids :: "nat \<Rightarrow> nat \<Rightarrow> nat list" where
  "block_ids b g \<equiv> map (\<lambda>t. gid b g t) [0..<b]"

fun launch_of :: "nat \<Rightarrow> nat \<Rightarrow> nat list" where
  "launch_of 0 b = []"
| "launch_of (Suc G) b = launch_of G b @ block_ids b G"

definition launch :: "nat \<Rightarrow> nat \<Rightarrow> nat list" where
  "launch n b \<equiv> launch_of (groups n b) b"

text \<open>The decomposition the group count is defined by: the dividend is its quotient
  times the divisor plus the remainder.\<close>

lemma groups_decomp:
  "groups n b * b + (n + b - 1) mod b = n + b - 1"
proof -
  have base: "(n + b - 1) div b * b + (n + b - 1) mod b = n + b - 1"
  proof -
    have h: "b * ((n + b - 1) div b) + (n + b - 1) mod b = n + b - 1"
      using div_mult_mod_eq [of "n + b - 1" b] by simp
    have e: "(n + b - 1) div b * b = b * ((n + b - 1) div b)"
      by (rule mult.commute)
    from h e show ?thesis by simp
  qed
  show ?thesis using base by (simp add: groups_def)
qed

text \<open>COVERAGE: the ceil group count is enough. The remainder is below `b`, so the
  quotient times `b` falls short of the dividend by less than one group.\<close>

lemma groups_cover:
  assumes b: "0 < b"
  shows "n \<le> groups n b * b"
proof -
  have d: "groups n b * b + (n + b - 1) mod b = n + b - 1"
    by (rule groups_decomp)
  have r: "(n + b - 1) mod b < b" using b by (rule mod_less_divisor)
  have rle: "(n + b - 1) mod b \<le> b - 1" using r b by arith
  show ?thesis using d rle b by arith
qed

text \<open>MINIMALITY: and not more than one group beyond what the work needs — the last
  group is partial at worst. A floor division fails the previous lemma; a ceil that added
  a whole extra group would fail this one.\<close>

lemma groups_tight:
  assumes b: "0 < b"
  shows "groups n b * b < n + b"
proof -
  have d: "groups n b * b + (n + b - 1) mod b = n + b - 1"
    by (rule groups_decomp)
  have "groups n b * b \<le> n + b - 1" using d by arith
  with b show ?thesis by arith
qed

lemma mult_suc_left: "Suc G * b = G * b + b"
proof -
  have "Suc G * b = b + G * b" by (simp add: mult_Suc)
  also have "\<dots> = G * b + b" by (rule add.commute)
  finally show ?thesis .
qed

lemma block_ids_length [simp]: "length (block_ids b g) = b"
  by (simp add: block_ids_def)

text \<open>Blocks are separated by magnitude: every id of an earlier block is smaller than
  every id of a later one. The whole of block `g` sits below `Suc g * b`.\<close>

lemma block_ids_lt:
  assumes gg: "g < g'" and tb: "t < b"
  shows "gid b g t < gid b g' t'"
proof -
  have sg: "Suc g \<le> g'" using gg by simp
  have mono: "Suc g * b \<le> g' * b" using sg by (rule mult_le_mono1)
  have less: "gid b g t < Suc g * b"
  proof -
    have "g * b + t < g * b + b" using tb by simp
    moreover have "g * b + b = Suc g * b" using mult_suc_left [of g b] by simp
    ultimately show ?thesis unfolding gid_def by arith
  qed
  have ge: "g' * b \<le> gid b g' t'" unfolding gid_def by simp
  from less mono ge show ?thesis by arith
qed

lemma block_ids_distinct: "distinct (block_ids b g)"
proof -
  have inj: "inj_on (\<lambda>t. gid b g t) (set [0..<b])"
  proof (rule inj_onI)
    fix x y
    assume "x \<in> set [0..<b]" and "y \<in> set [0..<b]"
    assume eq: "gid b g x = gid b g y"
    show "x = y" using eq unfolding gid_def by arith
  qed
  have eq: "distinct (map (\<lambda>t. gid b g t) [0..<b])
              = (distinct [0..<b] \<and> inj_on (\<lambda>t. gid b g t) (set [0..<b]))"
    by (rule distinct_map)
  have both: "distinct [0..<b] \<and> inj_on (\<lambda>t. gid b g t) (set [0..<b])"
  proof
    show "distinct [0..<b]" by simp
    show "inj_on (\<lambda>t. gid b g t) (set [0..<b])" by (rule inj)
  qed
  from eq both show ?thesis unfolding block_ids_def by simp
qed

text \<open>Every id a block emits is `g * b + t` for some `t < b`, which is the only shape the
  geometry arguments need. Stated once, so that each use is an `obtain` from a named fact
  rather than an `auto` that has to rediscover it.\<close>

lemma in_block_ids:
  assumes "x \<in> set (block_ids b g)"
  shows "\<exists>t. t < b \<and> x = g * b + t"
proof -
  from assms have im: "x \<in> (\<lambda>t. gid b g t) ` set [0..<b]"
    by (simp add: block_ids_def set_map)
  then obtain t where t: "t \<in> set [0..<b]" "x = gid b g t" by blast
  hence tb: "t < b" by simp
  from t tb show ?thesis unfolding gid_def by blast
qed

lemma launch_length: "length (launch_of G b) = G * b"
proof (induct G)
  case 0
  show ?case by simp
next
  case (Suc G)
  have "length (launch_of (Suc G) b) = length (launch_of G b) + b"
    by (simp add: block_ids_length)
  also have "\<dots> = G * b + b" using Suc.hyps by simp
  also have "\<dots> = Suc G * b" using mult_suc_left [of G b] by simp
  finally show ?case .
qed

lemma launch_bounds: "\<forall>x \<in> set (launch_of G b). x < G * b"
proof (induct G)
  case 0
  show ?case by simp
next
  case (Suc G)
  have ih: "\<forall>x \<in> set (launch_of G b). x < G * b" by (rule Suc.hyps)
  show ?case
  proof (rule ballI)
    fix x
    assume xm: "x \<in> set (launch_of (Suc G) b)"
    have split: "x \<in> set (launch_of G b) \<or> x \<in> set (block_ids b G)"
      using xm by simp
    from split show "x < Suc G * b"
    proof
      assume "x \<in> set (launch_of G b)"
      hence lt: "x < G * b" using ih by blast
      have "G * b \<le> Suc G * b" using mult_suc_left [of G b] by arith
      with lt show ?thesis by arith
    next
      assume xb: "x \<in> set (block_ids b G)"
      from xb obtain t where tb: "t < b" and teq: "x = G * b + t"
        using in_block_ids by blast
      have "x < G * b + b" using teq tb by arith
      moreover have "G * b + b = Suc G * b" using mult_suc_left [of G b] by simp
      ultimately show ?thesis by arith
    qed
  qed
qed

text \<open>Duplicate-freedom: block `G` is internally distinct and disjoint from everything
  launched before it (every earlier id is below `G * b`, every id of block `G` is at
  least `G * b`).\<close>

lemma distinct_launch_of: "distinct (launch_of G b)"
proof (induct G)
  case 0
  show ?case by simp
next
  case (Suc G)
  have ih: "distinct (launch_of G b)" by (rule Suc.hyps)
  have d: "distinct (block_ids b G)" by (rule block_ids_distinct)
  have disj: "set (launch_of G b) \<inter> set (block_ids b G) = {}"
  proof (rule equals0I)
    fix x
    assume x: "x \<in> set (launch_of G b) \<inter> set (block_ids b G)"
    have lt: "x < G * b"
    proof -
      have "x \<in> set (launch_of G b)" using x by simp
      thus ?thesis using launch_bounds [of G b] by blast
    qed
    have ge: "G * b \<le> x"
    proof -
      have xb: "x \<in> set (block_ids b G)" using x by simp
      from xb obtain t where tb: "t < b" and teq: "x = G * b + t"
        using in_block_ids by blast
      show ?thesis using teq by simp
    qed
    from lt ge show False by arith
  qed
  have both: "distinct (launch_of G b) \<and> distinct (block_ids b G)
                \<and> set (launch_of G b) \<inter> set (block_ids b G) = {}"
  proof
    show "distinct (launch_of G b)" by (rule ih)
    show "distinct (block_ids b G) \<and> set (launch_of G b) \<inter> set (block_ids b G) = {}"
    proof
      show "distinct (block_ids b G)" by (rule d)
      show "set (launch_of G b) \<inter> set (block_ids b G) = {}" by (rule disj)
    qed
  qed
  have eq: "distinct (launch_of G b @ block_ids b G)
              = (distinct (launch_of G b) \<and> distinct (block_ids b G)
                 \<and> set (launch_of G b) \<inter> set (block_ids b G) = {})"
    by (rule distinct_append)
  have "distinct (launch_of G b @ block_ids b G)" using eq both by simp
  thus ?case by simp
qed

text \<open>TILING: the ids of a launch of `G` groups of `b` threads are exactly the slots
  `0` to `G * b - 1`, each once — cardinality in both directions: strictly bounded
  above by `G * b`, duplicate-free, and `G * b` of them.\<close>

theorem launch_tiles: "set (launch_of G b) = {..< G * b}"
proof (rule equalityI)
  show "set (launch_of G b) \<subseteq> {..< G * b}"
  proof
    fix x
    assume xm: "x \<in> set (launch_of G b)"
    have "x < G * b" using xm launch_bounds [of G b] by blast
    thus "x \<in> {..< G * b}" by simp
  qed
next
  have cardL: "card (set (launch_of G b)) = G * b"
  proof -
    have "card (set (launch_of G b)) = length (launch_of G b)"
      using distinct_launch_of by (rule distinct_card)
    also have "\<dots> = G * b" by (rule launch_length)
    finally show ?thesis .
  qed
  have cardI: "card {..< G * b} = G * b" by simp
  have finI: "finite {..< G * b}" by simp
  show "{..< G * b} \<subseteq> set (launch_of G b)"
  proof
    fix i
    assume im: "i \<in> {..< G * b}"
    hence ilt: "i < G * b" by simp
    show "i \<in> set (launch_of G b)"
    proof (rule ccontr)
      assume nin: "i \<notin> set (launch_of G b)"
      have fS: "finite (set (launch_of G b))" by simp
      have card_ins: "card (insert i (set (launch_of G b)))
                        = Suc (card (set (launch_of G b)))"
        using nin fS by (simp add: card_insert_if)
      have sub2: "insert i (set (launch_of G b)) \<subseteq> {..< G * b}"
      proof
        fix y
        assume ym: "y \<in> insert i (set (launch_of G b))"
        have "y < G * b"
        proof (cases "y = i")
          case True
          thus ?thesis using ilt by simp
        next
          case False
          with ym have "y \<in> set (launch_of G b)" by simp
          thus ?thesis using launch_bounds [of G b] by blast
        qed
        thus "y \<in> {..< G * b}" by simp
      qed
      have le: "card (insert i (set (launch_of G b))) \<le> card {..< G * b}"
        using finI sub2 by (rule card_mono)
      have "Suc (G * b) \<le> G * b" using card_ins cardL cardI le by simp
      thus False by simp
    qed
  qed
qed

text \<open>And therefore the work is dispatched: coverage for the node's own `launch`, and —
  with the distinctness above — exactly once.\<close>

theorem launch_covers:
  assumes b: "0 < b"
  shows "{..<n} \<subseteq> set (launch n b)"
proof
  fix i
  assume "i \<in> {..<n}"
  hence i: "i < n" by simp
  have g: "n \<le> groups n b * b" using b by (rule groups_cover)
  have t: "set (launch n b) = {..< groups n b * b}"
    by (simp add: launch_def launch_tiles)
  from i g have "i < groups n b * b" by arith
  with t show "i \<in> set (launch n b)" by simp
qed

theorem launch_covers_exactly_once:
  assumes b: "0 < b" and i: "i < n"
  shows "i \<in> set (launch n b) \<and> distinct (launch n b)"
proof
  show "i \<in> set (launch n b)" using launch_covers b i by auto
  show "distinct (launch n b)" unfolding launch_def by (rule distinct_launch_of)
qed


section \<open>GpuOp: witnesses, and the weaker models this refuses\<close>

text \<open>The witness that is worth having is the NON-DIVISIBLE launch: 10 work items on
  4 threads needs 3 groups, the last of which is half empty. Every edge case of the
  geometry is present in it — a partial last group, a thread id in the last group
  that no work item owns, and the exact count `3 * 4`. The empty launch, a launch well inside one group, and the
  one-thread (b = 1) cases are the boundary.\<close>

lemma gpu_launch_witness:
  "groups 10 4 = 3
   \<and> launch 10 4 = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
   \<and> length (launch 10 4) = 12
   \<and> length (filter (\<lambda>x. x = 9) (launch 10 4)) = 1"
  by eval

lemma gpu_launch_empty_and_single:
  "launch 0 4 = [] \<and> launch 1 4 = [0, 1, 2, 3] \<and> launch 5 1 = [0, 1, 2, 3, 4]"
  by eval

text \<open>First weaker model: FLOOR rounding. It is the obvious mistake (a full group per
  `b` items, remainder dropped), it satisfies "no duplicates" and "no wasted group",
  and it silently loses the tail of the work.\<close>

definition launch_floor :: "nat \<Rightarrow> nat \<Rightarrow> nat list" where
  "launch_floor n b \<equiv> launch_of (n div b) b"

lemma floor_tiles_only_the_full_groups:
  "set (launch_floor n b) = {..< (n div b) * b}"
  by (simp add: launch_floor_def launch_tiles)

lemma floor_decomp: "(n::nat) = (n div b) * b + n mod b"
proof -
  have h: "b * (n div b) + n mod b = n"
    using div_mult_mod_eq [of n b] by simp
  have e: "(n div b) * b = b * (n div b)" by (rule mult.commute)
  from h e show ?thesis by simp
qed

theorem floor_rounding_is_not_a_launch:
  assumes b: "0 < b" and r: "n mod b \<noteq> 0"
  shows "\<exists>i < n. i \<notin> set (launch_floor n b)"
proof -
  let ?q = "(n div b) * b"
  have dec: "n = ?q + n mod b" by (rule floor_decomp)
  have rpos: "0 < n mod b" using r by simp
  have lt: "?q < n" using dec rpos by arith
  have tiles: "set (launch_floor n b) = {..< ?q}"
    by (rule floor_tiles_only_the_full_groups)
  have "?q \<notin> set (launch_floor n b)" using tiles lt by simp
  with lt show ?thesis by blast
qed

text \<open>Second weaker model: a block STRIDE of `b + 1` (the classic off-by-one in the
  global-id formula). It produces no duplicates and the right count, and leaves a
  hole at the end of every block but the last.\<close>

definition gid_stride :: "nat \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> nat" where
  "gid_stride b g t \<equiv> g * (Suc b) + t"

definition launch_stride :: "nat \<Rightarrow> nat \<Rightarrow> nat list" where
  "launch_stride n b \<equiv>
     concat (map (\<lambda>g. map (\<lambda>t. gid_stride b g t) [0..<b]) [0..< groups n b])"

lemma launch_stride_ids:
  assumes "x \<in> set (launch_stride n b)"
  shows "\<exists>g t. t < b \<and> x = g * Suc b + t"
proof -
  from assms obtain g t where gG: "g < groups n b" and tb: "t < b"
    and xe: "x = gid_stride b g t"
    unfolding launch_stride_def by auto
  from tb xe show ?thesis unfolding gid_stride_def by blast
qed

theorem stride_hole_is_not_a_launch:
  assumes b: "0 < b" and nb: "b < n"
  shows "b \<notin> set (launch_stride n b) \<and> b < n"
proof
  show "b < n" by (rule nb)
  show "b \<notin> set (launch_stride n b)"
  proof
    assume bin: "b \<in> set (launch_stride n b)"
    from bin obtain g t where tb: "t < b" and xe: "b = g * Suc b + t"
      using launch_stride_ids by blast
    show False
    proof (cases g)
      case 0
      with xe have "b = t" by simp
      with tb show ?thesis by simp
    next
      case (Suc g')
      with xe have eq: "b = Suc g' * Suc b + t" by simp
      have "Suc b \<le> Suc g' * Suc b" using mult_Suc [of g' "Suc b"] by simp
      hence "Suc b \<le> b" using eq by arith
      thus ?thesis by simp
    qed
  qed
qed

text \<open>Third weaker model: a launch that COVERS but repeats a thread id. This is why
  the contract is coverage AND duplicate-freedom rather than coverage alone — the
  weaker statement would accept a dispatch that runs one work item twice.\<close>

definition launch_dup :: "nat \<Rightarrow> nat \<Rightarrow> nat list" where
  "launch_dup n b \<equiv> 0 # launch n b"

lemma duplicate_launch_is_rejected:
  "distinct (launch 4 2) \<and> \<not> distinct (launch_dup 4 2)"
  by eval

theorem covering_alone_is_not_enough:
  "\<exists>n b. (0::nat) < b \<and> (\<forall>i. i < n \<longrightarrow> i \<in> set (launch_dup n b))
            \<and> \<not> distinct (launch_dup n b)"
proof (rule exI [of _ 4], rule exI [of _ 2], rule conjI)
  show "0 < (2::nat)" by simp
next
  show "(\<forall>i. i < 4 \<longrightarrow> i \<in> set (launch_dup 4 2)) \<and> \<not> distinct (launch_dup 4 2)"
  proof
    show "\<forall>i. i < 4 \<longrightarrow> i \<in> set (launch_dup 4 2)"
    proof (rule allI, rule impI)
      fix i :: nat
      assume "i < 4"
      with launch_covers [of 2 4] show "i \<in> set (launch_dup 4 2)"
        unfolding launch_dup_def by auto
    qed
    show "\<not> distinct (launch_dup 4 2)" using duplicate_launch_is_rejected by simp
  qed
qed

text \<open>The contract, collected in the form the node cites. What is NOT here is the
  device: no device heap, no buffer allocation or transfer, no synchronisation, no
  async completion order, and no claim about the kernel's own effect — the node
  enqueues work and its own observable value is a completion handle.\<close>

theorem gpu_op_sound:
  shows "path_join (device # slot) verb = device @ [47] @ path_join slot verb"
    and "path_join segs (verb @ w) = path_join segs verb @ w"
    and "0 < b \<Longrightarrow> n \<le> groups n b * b"
    and "0 < b \<Longrightarrow> groups n b * b < n + b"
    and "set (launch_of G b) = {..< G * b}"
    and "0 < b \<Longrightarrow> {..<n} \<subseteq> set (launch n b)"
    and "0 < b \<Longrightarrow> distinct (launch n b)"
    and "0 < b \<Longrightarrow> i < n \<Longrightarrow> i \<in> set (launch n b) \<and> distinct (launch n b)"
proof -
  show "path_join (device # slot) verb = device @ [47] @ path_join slot verb"
    by (rule gpu_path_last_segment)
  show "path_join segs (verb @ w) = path_join segs verb @ w"
    by (rule gpu_path_verb_is_the_tail)
  show "0 < b \<Longrightarrow> n \<le> groups n b * b" by (rule groups_cover)
  show "0 < b \<Longrightarrow> groups n b * b < n + b" by (rule groups_tight)
  show "set (launch_of G b) = {..< G * b}" by (rule launch_tiles)
  show "0 < b \<Longrightarrow> {..<n} \<subseteq> set (launch n b)" by (rule launch_covers)
  show "0 < b \<Longrightarrow> distinct (launch n b)"
    unfolding launch_def by (rule distinct_launch_of)
  show "0 < b \<Longrightarrow> i < n \<Longrightarrow> i \<in> set (launch n b) \<and> distinct (launch n b)"
    by (rule launch_covers_exactly_once)
qed


section \<open>CgoCall: the argument layout of a native call\<close>

text \<open>A `CgoCall` is a call into a CGO-bound native library — the faithful
  translation is the native call itself, so the node has no value of its own: it
  fixes the CALLEE (a dotted target, rendered verbatim) and the ARGUMENTS. What a
  node like this can be wrong about is the argument layout, and that is a property of
  the node rather than of the foreign function: the arguments are passed positionally,
  in declaration order, each exactly once, each occupying exactly its declared width,
  with no padding and no overlap.

  The model below is deliberately absolute: an argument is a `(width, blob)` pair in
  the theory only in the sense that the layout is a concatenation of blobs and the
  offsets are the sums of the earlier blobs. The declared WIDTH is not used to
  compute anything — the node does not marshal values, it passes them — which is why
  the last theorem in this section shows that the layout alone does not recover the
  argument list: the widths are part of the contract even though they are not part
  of the bytes.

  Nothing about the callee is claimed: the C type mapping beyond width, ownership and
  lifetime of returned pointers, the errno channel, and the call's side effects on
  process state. `result = marshal_out (C_fn (marshal_in args))` is the foreign
  function's contract, not this node's.\<close>

definition layout :: "(nat \<times> bytes) list \<Rightarrow> bytes" where
  "layout args \<equiv> concat (map snd args)"

definition arg_offset :: "(nat \<times> bytes) list \<Rightarrow> nat \<Rightarrow> nat" where
  "arg_offset args k \<equiv> sum_list (map (\<lambda>a. length (snd a)) (take k args))"

definition declared_width :: "(nat \<times> bytes) list \<Rightarrow> nat" where
  "declared_width args \<equiv> sum_list (map fst args)"

text \<open>EXACT SIZE: the buffer is the concatenation of the blobs, so its length is
  the sum of the blob lengths — no padding, no alignment rounding, no truncation.\<close>

lemma layout_length:
  "length (layout args) = sum_list (map (\<lambda>a. length (snd a)) args)"
  by (simp add: layout_def length_concat o_def)

lemma layout_append [simp]: "layout (args @ args') = layout args @ layout args'"
  unfolding layout_def by (simp add: map_append concat_append)

text \<open>OFFSETS: the offset of argument `k` is the sum of the blobs before it, and the
  recursion adds exactly the `k`-th blob.\<close>

lemma arg_offset_zero [simp]: "arg_offset args 0 = 0"
  unfolding arg_offset_def by simp

lemma arg_offset_cons:
  assumes k: "k < length args"
  shows "arg_offset args (Suc k) = arg_offset args k + length (snd (args ! k))"
proof -
  have "take (Suc k) args = take k args @ [args ! k]"
    using k by (simp add: take_Suc_conv_app_nth)
  thus ?thesis unfolding arg_offset_def by simp
qed

lemma layout_take_split:
  assumes k: "k \<le> length args"
  shows "layout args = layout (take k args) @ layout (drop k args)"
proof -
  have a: "take k args @ drop k args = args" using k by simp
  have "layout (take k args) @ layout (drop k args)
          = layout (take k args @ drop k args)"
    by (simp only: layout_append [symmetric])
  also have "\<dots> = layout args" by (simp only: a)
  finally show ?thesis ..
qed

text \<open>POSITION: dropping the bytes of the earlier arguments leaves exactly the later
  ones, in order. This is the law that says the layout is positional: the `k`-th argument
  starts at the sum of the previous blobs, whatever those blobs are.\<close>

lemma layout_drop_offset:
  assumes k: "k \<le> length args"
  shows "drop (arg_offset args k) (layout args) = concat (map snd (drop k args))"
proof -
  have lay: "layout args = layout (take k args) @ layout (drop k args)"
    by (rule layout_take_split [OF k])
  have off: "arg_offset args k = length (layout (take k args))"
  proof -
    have "arg_offset args k = sum_list (map (\<lambda>a. length (snd a)) (take k args))"
      by (simp add: arg_offset_def)
    also have "\<dots> = length (layout (take k args))"
      by (simp add: layout_def length_concat o_def)
    finally show ?thesis .
  qed
  have "drop (arg_offset args k) (layout args)
          = drop (length (layout (take k args)))
              (layout (take k args) @ layout (drop k args))"
    using lay off by simp
  also have "\<dots> = layout (drop k args)" by simp
  also have "\<dots> = concat (map snd (drop k args))" unfolding layout_def ..
  finally show ?thesis .
qed

text \<open>WINDOW: each argument occupies exactly its own bytes at its own offset, so taking
  the blob's length there returns that blob — the arguments do not interleave.\<close>

lemma layout_arg_window:
  assumes k: "k < length args"
  shows "take (length (snd (args ! k))) (drop (arg_offset args k) (layout args)) = snd (args ! k)"
proof -
  have d: "drop (arg_offset args k) (layout args) = concat (map snd (drop k args))"
  proof -
    have "k \<le> length args" using k by arith
    thus ?thesis by (rule layout_drop_offset)
  qed
  have split: "drop k args = args ! k # drop (Suc k) args"
    using Cons_nth_drop_Suc [OF k, symmetric] .
  have "concat (map snd (drop k args))
          = snd (args ! k) @ concat (map snd (drop (Suc k) args))"
    using split by simp
  with d show ?thesis by simp
qed

text \<open>NOTHING DROPPED: an empty layout means every blob was empty — an argument with a
  non-empty blob cannot vanish from the concatenation.\<close>

lemma layout_nil_all_blobs_nil:
  assumes "layout args = []"
  shows "\<forall>a \<in> set args. snd a = []"
  using assms unfolding layout_def by (simp add: concat_eq_Nil_conv)


section \<open>CgoCall: witnesses, and the weaker models this refuses\<close>

text \<open>The witness that matters is the ZERO-WIDTH argument: it keeps its position in
  the list but consumes no bytes, so two consecutive offsets are equal. A layout that
  packed only the non-empty arguments, or that derived positions from a running
  non-zero counter, would get this wrong; the declaration order and the declared
  widths are what make the position right.\<close>

lemma cgo_layout_witness:
  "layout [(2, [1, 2]), (1, [3]), (0, [])] = [1, 2, 3]
   \<and> arg_offset [(2, [1, 2]), (1, [3]), (0, [])] 0 = 0
   \<and> arg_offset [(2, [1, 2]), (1, [3]), (0, [])] 1 = 2
   \<and> arg_offset [(2, [1, 2]), (1, [3]), (0, [])] 2 = 3
   \<and> arg_offset [(2, [1, 2]), (1, [3]), (0, [])] 3 = 3
   \<and> declared_width [(2, [1, 2]), (1, [3]), (0, [])] = 3"
  by eval

text \<open>The window and drop laws on the same three arguments, in the ground form a
  reviewer can read without the generality: argument 1 is `[3]` at offset 2, and
  dropping to offset 2 leaves exactly arguments 1 and 2.\<close>

lemma cgo_layout_position_witness:
  "take (length (snd ([(2, [1, 2]), (1, [3]), (0, [])] ! 1)))
        (drop (arg_offset [(2, [1, 2]), (1, [3]), (0, [])] 1)
              (layout [(2, [1, 2]), (1, [3]), (0, [])])) = [3]
   \<and> drop (arg_offset [(2, [1, 2]), (1, [3]), (0, [])] 1)
       (layout [(2, [1, 2]), (1, [3]), (0, [])]) = [3]"
  by (simp add: layout_def arg_offset_def)

text \<open>First weaker model: ORDER-BY-SIZE. Laying the arguments out largest first is a
  plausible "packing" optimisation and it silently swaps the positions of two
  arguments of different sizes.\<close>

lemma reordered_layout_differs:
  "layout [(1, [9]), (2, [1, 2])] \<noteq> layout [(2, [1, 2]), (1, [9])]"
  by eval

text \<open>Second weaker model: PAD each argument to a fixed size. It preserves order and
  positions-relative-to-padding, and it breaks the exact-size law, which is what makes
  a buffer overrun or a size check against `sizeof` disagree with the node.\<close>

definition layout_padded :: "(nat \<times> bytes) list \<Rightarrow> bytes" where
  "layout_padded args \<equiv> concat (map (\<lambda>a. snd a @ [0]) args)"

theorem padding_breaks_the_size_law:
  "length (layout_padded [(1, [97]), (1, [98])]) \<noteq> length (layout [(1, [97]), (1, [98])])"
  by eval

text \<open>Third weaker model: DROP THE LAST ARGUMENT (the variadic-call mistake). The
  remaining layout is a prefix of the right one, so every offset that survives is
  "correct" — and the callee reads the wrong arity.\<close>

definition layout_init :: "(nat \<times> bytes) list \<Rightarrow> bytes" where
  "layout_init args \<equiv> layout (butlast args)"

theorem dropping_an_argument_loses_its_bytes:
  "layout_init [(1, [97]), (1, [98])] = [97]
   \<and> layout [(1, [97]), (1, [98])] = [97, 98]"
  by eval

lemma layout_init_short:
  "length (layout_init [(1, [97]), (1, [98])]) \<noteq> length (layout [(1, [97]), (1, [98])])"
  by eval

text \<open>And the reason the widths are part of the contract rather than decoration: the
  same bytes are two one-byte arguments or one two-byte argument, so a layout without
  the declared widths does not determine what the callee was handed.\<close>

theorem layout_alone_is_not_the_argument_list:
  "layout [(1, [97]), (1, [98])] = layout [(2, [97, 98])]
   \<and> ([(1, [97]), (1, [98])] :: (nat \<times> bytes) list) \<noteq> [(2, [97, 98])]"
  by eval

text \<open>The contract, collected in the form the node cites.\<close>

theorem cgo_call_sound:
  shows "length (layout args) = sum_list (map (\<lambda>a. length (snd a)) args)"
    and "k < length args \<Longrightarrow>
           arg_offset args (Suc k) = arg_offset args k + length (snd (args ! k))"
    and "k \<le> length args \<Longrightarrow>
           layout args = layout (take k args) @ layout (drop k args)"
    and "k \<le> length args \<Longrightarrow>
           drop (arg_offset args k) (layout args) = concat (map snd (drop k args))"
    and "k < length args \<Longrightarrow>
           take (length (snd (args ! k))) (drop (arg_offset args k) (layout args))
             = snd (args ! k)"
    and "layout (args @ args') = layout args @ layout args'"
    and "layout args = [] \<Longrightarrow> \<forall>a \<in> set args. snd a = []"
proof -
  show "length (layout args) = sum_list (map (\<lambda>a. length (snd a)) args)"
    by (rule layout_length)
  show "k < length args \<Longrightarrow>
           arg_offset args (Suc k) = arg_offset args k + length (snd (args ! k))"
    by (rule arg_offset_cons)
  show "k \<le> length args \<Longrightarrow>
           layout args = layout (take k args) @ layout (drop k args)"
    by (rule layout_take_split)
  show "k \<le> length args \<Longrightarrow>
           drop (arg_offset args k) (layout args) = concat (map snd (drop k args))"
    by (rule layout_drop_offset)
  show "k < length args \<Longrightarrow>
           take (length (snd (args ! k))) (drop (arg_offset args k) (layout args))
             = snd (args ! k)"
    by (rule layout_arg_window)
  show "layout (args @ args') = layout args @ layout args'"
    by (rule layout_append)
  show "layout args = [] \<Longrightarrow> \<forall>a \<in> set args. snd a = []"
    by (rule layout_nil_all_blobs_nil)
qed


section \<open>What is settled, and what belongs to the device or the callee\<close>

text \<open>GpuOp: the path the write denotes is the join of its segments and the verb is
  the last of them (`gpu_path_last_segment`); a launch covers its work with ceil
  groups (`groups_cover`), adds no gratuitous group (`groups_tight`), tiles the grid
  exactly (`launch_tiles`) and therefore dispatches every work item exactly once
  (`launch_covers_exactly_once`). Refuted: floor rounding
  (`floor_rounding_is_not_a_launch`), a `b + 1` block stride
  (`stride_hole_is_not_a_launch`), and coverage without duplicate-freedom
  (`covering_alone_is_not_enough`).

  CgoCall: the argument buffer is the concatenation of the blobs, its size the sum of
  their lengths (`layout_length`), each argument
  sits at the sum of the earlier blobs (`arg_offset_cons`, `layout_drop_offset`) and
  occupies exactly its own bytes there (`layout_arg_window`); order is load-bearing
  (`reordered_layout_differs`), padding breaks the size law
  (`padding_breaks_the_size_law`), dropping an argument loses its bytes
  (`dropping_an_argument_loses_its_bytes`), and the layout alone is not the argument
  list (`layout_alone_is_not_the_argument_list`).

  Left to the device and the callee, deliberately: for `GpuOp`, device memory
  (allocation, transfers), synchronisation and async completion order, and the
  kernel's own effect — `out_buffer = kernel(in_buffer)` is the kernel's contract.
  For `CgoCall`, the C ABI type mapping beyond width, ownership and lifetime of
  returned pointers, the errno/error channel, and side effects on process state.
  Neither is a byte-level property of the node, and neither is claimed.\<close>

end

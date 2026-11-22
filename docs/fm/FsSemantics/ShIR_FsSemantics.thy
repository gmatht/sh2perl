(*  FsSemantics — the meaning of the three stat/identity predicates.

      FsSameFile            `[ A -ef B ]`   (shir_nodes/fs_same_file.node)
      FsNewerThan           `[ A -nt B ]`   (shir_nodes/fs_newer_than.node)
      FsModifiedSinceRead   `[ -N f ]`      (shir_nodes/fs_modified_since_read.node)

    The node files end in `spec: FsSemantics#<name>`; this session is what makes
    those lines resolve, and the obligations are the DOCUMENTED rules:

      same_file            exists A AND exists B AND dev(A)=dev(B)
                           AND ino(A)=ino(B) — two missing paths are NOT the
                           same file, and the relation is symmetric.
      newer_than           A -nt B iff exists A and (B missing or mtime(A) >
                           mtime(B)); A -ot B iff exists B and (A missing or
                           mtime(A) < mtime(B)); the two are never both true,
                           and both false when BOTH are missing.
      modified_since_read  -N f iff exists f and mtime(f) > atime(f) — both
                           times from ONE stat, and equal timestamps are false.

    A state is a LIST of (path, metadata) pairs read through `map_of`, not a
    function: a function literal cannot be reduced by the code generator, so
    every `by eval` pin would be a proof obligation nobody could discharge.
    The denotations are the `*_sem` functions; each spec names the THEOREM
    about it, because a definition cannot be an obligation.

    The `by eval` pins quote MEASURED bash answers on this box (see the `text`
    blocks); a model change that stops matching bash fails the build rather
    than the corpus.

    Build:  isabelle build -D sh2perl/docs/fm/FsSemantics
*)

theory ShIR_FsSemantics
  imports Main
begin

section \<open>One state, as stat(2) reports it\<close>

text \<open>dev/ino identify a file; mtime/atime are in the same unit (nanoseconds on
  a modern filesystem — the node refuses a seconds comparison precisely because
  it turns "written in the same second" into "not newer").\<close>

datatype meta = Meta (dev: nat) (ino: nat) (mtime: nat) (atime: nat)

type_synonym state = "(string \<times> meta) list"

definition lookup :: "state \<Rightarrow> string \<Rightarrow> meta option" where
  "lookup st p = map_of st p"

definition exists_at :: "state \<Rightarrow> string \<Rightarrow> bool" where
  "exists_at st p \<longleftrightarrow> (lookup st p \<noteq> None)"

section \<open>`-ef` (FsSameFile)\<close>

definition same_file_sem :: "state \<Rightarrow> string \<Rightarrow> string \<Rightarrow> bool" where
  "same_file_sem st a b \<longleftrightarrow>
     (case (lookup st a, lookup st b) of
        (Some x, Some y) \<Rightarrow> dev x = dev y \<and> ino x = ino y
      | _ \<Rightarrow> False)"

text \<open>`same_file` — the obligation `fs_same_file.node` cites.  Symmetry is the
  content the two renderers share: whichever side the emitter passes first is
  the same question.  (Order IS load-bearing for the diagnostics, which is why
  the node keeps both fields; it is not load-bearing for the answer.)\<close>

theorem same_file: "same_file_sem st a b = same_file_sem st b a"
  by (auto simp: same_file_sem_def split: option.splits prod.splits)

lemma same_file_carries_presence: "same_file_sem st a b \<Longrightarrow> exists_at st a"
  by (cases "lookup st a"; cases "lookup st b";
      simp_all add: same_file_sem_def exists_at_def)

lemma missing_is_not_the_same_file:
  "lookup st a = None \<Longrightarrow> lookup st b = None \<Longrightarrow> \<not> same_file_sem st a b"
  by (simp add: same_file_sem_def)

text \<open>Measured on this box: `[ missing -ef missing ]` is false (two missing
  operands are not "the same"), while `ln -s f l; [ f -ef l ]` is TRUE — the
  predicate is defined over stat(2), so it follows the symlink, which in the
  model is two paths carrying the same dev/ino.\<close>

lemma same_file_follows_symlink:
  "same_file_sem [(''f'', Meta 8 42 100 100), (''l'', Meta 8 42 100 100)] ''f'' ''l''"
  by eval

lemma same_file_different_inodes:
  "\<not> same_file_sem [(''f'', Meta 8 42 100 100), (''g'', Meta 8 43 100 100)] ''f'' ''g''"
  by eval

lemma same_file_missing_operand:
  "\<not> same_file_sem [(''f'', Meta 8 42 100 100)] ''f'' ''missing''"
  by eval

section \<open>`-nt` / `-ot` (FsNewerThan)\<close>

definition newer_sem :: "state \<Rightarrow> string \<Rightarrow> string \<Rightarrow> bool" where
  "newer_sem st a b \<longleftrightarrow>
     (case lookup st a of
        None \<Rightarrow> False
      | Some x \<Rightarrow> (case lookup st b of None \<Rightarrow> True | Some y \<Rightarrow> mtime x > mtime y))"

definition older_sem :: "state \<Rightarrow> string \<Rightarrow> string \<Rightarrow> bool" where
  "older_sem st a b \<longleftrightarrow>
     (case lookup st b of
        None \<Rightarrow> False
      | Some y \<Rightarrow> (case lookup st a of None \<Rightarrow> True | Some x \<Rightarrow> mtime x < mtime y))"

text \<open>`newer_than` — the obligation `fs_newer_than.node` cites.  The
  missing-operand rule and the exclusivity are the two facts a renderer that
  spells `mtime(A) > mtime(B)` without the presence checks gets wrong: both
  missing is neither -nt nor -ot, and A missing with B present is -ot only.\<close>

theorem newer_than: "\<not> (newer_sem st a b \<and> older_sem st a b)"
  by (cases "lookup st a"; cases "lookup st b";
      auto simp: newer_sem_def older_sem_def intro: less_asym)

lemma both_missing_is_neither_newer_nor_older:
  "lookup st a = None \<Longrightarrow> lookup st b = None \<Longrightarrow> \<not> newer_sem st a b"
  by (simp add: newer_sem_def)

lemma missing_right_is_newer:
  "lookup st a = Some x \<Longrightarrow> lookup st b = None \<Longrightarrow> newer_sem st a b"
  by (simp add: newer_sem_def)

lemma missing_left_is_older:
  "lookup st a = None \<Longrightarrow> lookup st b = Some y \<Longrightarrow> older_sem st a b"
  by (simp add: older_sem_def)

text \<open>Measured: a file one NANOSECOND newer is `-nt` true; two files with the
  same timestamp are not (the node exists because a seconds comparison gets
  this wrong).\<close>

lemma newer_than_nanoseconds:
  "newer_sem [(''a'', Meta 8 1 1000001 0), (''b'', Meta 8 2 1000000 0)] ''a'' ''b''"
  by eval

lemma same_timestamp_is_not_newer:
  "\<not> newer_sem [(''a'', Meta 8 1 1000000 0), (''b'', Meta 8 2 1000000 0)] ''a'' ''b''"
  by eval

lemma both_missing_newer_pin:
  "\<not> newer_sem [] ''a'' ''b''"
  by eval

section \<open>`-N` (FsModifiedSinceRead)\<close>

definition modified_sem :: "state \<Rightarrow> string \<Rightarrow> bool" where
  "modified_sem st p \<longleftrightarrow>
     (case lookup st p of None \<Rightarrow> False | Some x \<Rightarrow> mtime x > atime x)"

text \<open>`modified_since_read` — the obligation
  `fs_modified_since_read.node` cites.  Stated with ONE witness on purpose:
  mtime and atime come from the same stat, which is the difference between
  `-N f` and a two-stat "mtime newer than atime?" spelling.  A missing path is
  false with no diagnostic, and equal times are false.\<close>

theorem modified_since_read:
  "modified_sem st p \<longleftrightarrow> (\<exists>x. lookup st p = Some x \<and> mtime x > atime x)"
  by (cases "lookup st p"; simp_all add: modified_sem_def)

lemma missing_is_not_modified: "lookup st p = None \<Longrightarrow> \<not> modified_sem st p"
  by (simp add: modified_sem_def)

lemma equal_times_are_not_modified:
  "modified_sem st p \<Longrightarrow> lookup st p = Some x \<Longrightarrow> mtime x \<noteq> atime x"
  by (simp add: modified_sem_def)

lemma modified_after_write:
  "modified_sem [(''f'', Meta 8 1 200 100)] ''f''"
  by eval

lemma read_file_is_not_modified:
  "\<not> modified_sem [(''f'', Meta 8 1 100 100)] ''f''"
  by eval

end

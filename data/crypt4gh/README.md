# Crypt4GH example file

This is just a customised summary for htsget-rs. Please refer to the official [`crypt4gh-rust` documentation](https://ega-archive.github.io/crypt4gh-rust) for further information.

## Keygen

```sh
cargo install crypt4gh
crypt4gh keygen --sk keys/alice.sec --pk keys/alice.pub
crypt4gh keygen --sk keys/bob.sec --pk keys/bob.pub
```

## Encrypt
```
crypt4gh encrypt --sk keys/alice.sec --recipient_pk keys/bob.pub < htsnexus_test_NA12878.bam > htsnexus_test_NA12878.bam.c4gh
```

## Decrypt

```sh
crypt4gh decryptor --range 0-65535 --sk data/crypt4gh/keys/bob.sec \
                                 --sender-pk data/crypt4gh/keys/alice.pub \
                                 < data/crypt4gh/htsnexus_test_NA12878.bam.c4gh \
                                 > out.bam

samtools view out.bam
(...)
SRR098401.61822403	83	11	5009470	60	76M	=	5009376	-169	TCTTCTTGCCCTGGTGTTTCGCCGTTCCAGTGCCCCCTGCTGCAGACCATAAAGGATGGGACTTTGTTGAGGTAGG	?B6BDCD@I?JFI?FHHFEAIIAHHDIJHHFIIIIIJEIIFIJGHCIJDDEEHHHDEHHHCIGGEGFDGFGFBEDC	X0:i:1	X1:i:0	MD:Z:76	RG:Z:SRR098401	AM:i:37	NM:i:0	SM:i:37	MQ:i:60	XT:A:U	BQ:Z:@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@B

samtools view: error reading file "out.bam"
samtools view: error closing "out.bam": -1
```

The last samtools view error suggests that the returned bytes do not include BAM file termination.

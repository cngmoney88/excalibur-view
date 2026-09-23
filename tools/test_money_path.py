#!/usr/bin/env python3
"""The path from a paid order to a server holding the licence.

Run it: python tools/test_money_path.py

No Square, no signing key, no network. What it covers is the part that was
missing and the part most likely to drift: where a licence is written, under
what name, and whether pushing it actually happens. Signing itself is tested
elsewhere and needs the key on the signing machine.
"""
import json
import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import publish  # noqa: E402
import square  # noqa: E402


def a_licence(id="evl_abc123", company="Mesa Fab, Inc.", users=6):
    return {
        "id": id,
        "company": company,
        "users": users,
        "edition": "office",
        "updates_through": "2027-09-24",
        "signature": "not a real one, and this test does not need one",
    }


class WhereALicenceIsPublished(unittest.TestCase):
    def test_the_name_is_the_one_the_server_asks_for(self):
        # Pinned on the Rust side too, in license.rs. If these ever drift
        # apart every server quietly stops finding its own renewal and nobody
        # gets an error to read.
        self.assertEqual(
            publish.license_filename_for("evl_abc123"),
            "19431c5846060c7cb52d58f6af8352ed35243c827a01b2703ca4c850160aaebd.evlicense",
        )

    def test_a_stray_space_is_not_a_lost_licence(self):
        self.assertEqual(
            publish.license_filename_for(" evl_abc123 "),
            publish.license_filename_for("evl_abc123"),
        )

    def test_the_company_name_cannot_be_worked_out_from_the_address(self):
        one = publish.license_filename_for("evl_mesa_fab")
        two = publish.license_filename_for("evl_mesa_fab_2")
        self.assertNotIn("mesa", one)
        self.assertNotEqual(one, two)

    def test_publishing_writes_it_where_a_server_will_look(self):
        with tempfile.TemporaryDirectory() as feed:
            lic = a_licence()
            out = square.publish_into(lic, feed, quiet=True)
            self.assertTrue(os.path.exists(out))
            self.assertEqual(
                os.path.basename(out),
                publish.license_filename_for(lic["id"]),
            )
            with open(out, encoding="utf-8") as f:
                self.assertEqual(json.load(f), lic)

    def test_a_renewal_replaces_the_one_that_was_there(self):
        # Selling three more seats publishes the same id at a higher count.
        # It has to land on the same name, or the server keeps fetching the
        # old one and the seats never arrive.
        with tempfile.TemporaryDirectory() as feed:
            first = square.publish_into(a_licence(users=6), feed, quiet=True)
            second = square.publish_into(a_licence(users=9), feed, quiet=True)
            self.assertEqual(first, second)
            with open(second, encoding="utf-8") as f:
                self.assertEqual(json.load(f)["users"], 9)
            self.assertEqual(len(os.listdir(feed)), 1, "one file, not two")

    def test_the_folder_is_made_if_it_is_not_there(self):
        with tempfile.TemporaryDirectory() as parent:
            feed = os.path.join(parent, "static", "f")
            square.publish_into(a_licence(), feed, quiet=True)
            self.assertTrue(os.path.isdir(feed))


class GettingItBackWithoutAskingAnybody(unittest.TestCase):
    """A customer who lost the file has lost the licence id with it.

    What they still have is their Square receipt, so the same licence is
    published a second time under the payment id. A page in a browser can
    work that address out; nothing has to be looked up, so there is nothing
    to look somebody else's licence up with.
    """

    def test_the_receipt_address_matches_what_a_browser_would_work_out(self):
        # The page does: SHA-256("receipt:" + payment id). Pinned here,
        # because if the two drift apart the page finds nothing and says the
        # licence does not exist, which is the worst possible wrong answer.
        import hashlib
        payment = "PAYMENT123"
        want = hashlib.sha256(b"receipt:PAYMENT123").hexdigest() + ".evlicense"
        self.assertEqual(square.a_receipt_name(payment), want)

    def test_a_stray_space_off_a_receipt_still_finds_it(self):
        self.assertEqual(
            square.a_receipt_name("  PAYMENT123  "),
            square.a_receipt_name("PAYMENT123"),
        )

    def test_it_is_not_the_same_address_as_the_licence_id(self):
        # Two different things published under two different names, so
        # knowing one never gives you the other.
        self.assertNotEqual(
            square.a_receipt_name("evl_abc123"),
            publish.license_filename_for("evl_abc123"),
        )

    def test_the_receipt_copy_is_the_same_licence(self):
        with tempfile.TemporaryDirectory() as feed:
            lic = a_licence()
            square.publish_into(lic, feed, quiet=True, receipt="PAYMENT123")
            names = sorted(os.listdir(feed))
            self.assertEqual(len(names), 2, "one by id, one by receipt")
            for name in names:
                with open(os.path.join(feed, name), encoding="utf-8") as f:
                    self.assertEqual(json.load(f), lic)

    def test_nothing_is_published_by_receipt_when_there_is_no_receipt(self):
        # A licence signed by hand has no Square order behind it.
        with tempfile.TemporaryDirectory() as feed:
            square.publish_into(a_licence(), feed, quiet=True)
            self.assertEqual(len(os.listdir(feed)), 1)


class PushingIt(unittest.TestCase):
    """A licence on the signing machine has not reached anybody.

    This is the step that was missing, and the failure is silent: everything
    above succeeds, the file is there, and the customer's server never sees
    it because nothing was ever pushed.
    """

    def test_it_refuses_a_folder_that_is_not_a_checkout(self):
        with tempfile.TemporaryDirectory() as notrepo:
            with self.assertRaises(SystemExit):
                square.push_the_feed(notrepo, "nothing", quiet=True)

    def test_it_says_so_rather_than_committing_nothing(self):
        with tempfile.TemporaryDirectory() as repo:
            git = lambda *a: subprocess.run(["git", *a], cwd=repo, check=True,
                                            capture_output=True)
            git("init", "-q")
            git("config", "user.email", "t@t")
            git("config", "user.name", "t")
            os.makedirs(os.path.join(repo, "static", "f"))
            open(os.path.join(repo, "static", "f", ".keep"), "w").close()
            git("add", "-A")
            git("commit", "-q", "-m", "first")
            # Nothing has changed since, so there is nothing to push.
            self.assertFalse(square.push_the_feed(repo, "nothing new", quiet=True))

    def test_a_new_licence_is_committed(self):
        with tempfile.TemporaryDirectory() as repo:
            git = lambda *a: subprocess.run(["git", *a], cwd=repo, check=True,
                                            capture_output=True)
            git("init", "-q")
            git("config", "user.email", "t@t")
            git("config", "user.name", "t")
            feed = os.path.join(repo, "static", "f")
            os.makedirs(feed)
            open(os.path.join(feed, ".keep"), "w").close()
            git("add", "-A")
            git("commit", "-q", "-m", "first")

            square.publish_into(a_licence(), feed, quiet=True)
            # No remote here, so the push itself fails -- which is the point:
            # it must be loud rather than leaving somebody believing a licence
            # went out when it did not.
            with self.assertRaises(SystemExit):
                square.push_the_feed(repo, "Publish 1 licence", quiet=True)
            # But it was committed before the push was attempted.
            log = subprocess.run(["git", "log", "--oneline", "-1"], cwd=repo,
                                 capture_output=True, text=True).stdout
            self.assertIn("Publish 1 licence", log)

    def test_the_commit_message_never_names_a_customer(self):
        # A commit message is forever and the repository is shared. Whatever
        # else goes wrong, who bought what is not written into git history.
        with tempfile.TemporaryDirectory() as repo:
            git = lambda *a: subprocess.run(["git", *a], cwd=repo, check=True,
                                            capture_output=True)
            git("init", "-q")
            git("config", "user.email", "t@t")
            git("config", "user.name", "t")
            feed = os.path.join(repo, "static", "f")
            os.makedirs(feed)
            open(os.path.join(feed, ".keep"), "w").close()
            git("add", "-A")
            git("commit", "-q", "-m", "first")
            square.publish_into(a_licence(company="Mesa Fab, Inc."), feed, quiet=True)
            try:
                square.push_the_feed(repo, "Publish 1 licence", quiet=True)
            except SystemExit:
                pass
            log = subprocess.run(["git", "log", "--format=%s%n%b"], cwd=repo,
                                 capture_output=True, text=True).stdout
            self.assertNotIn("Mesa Fab", log)


if __name__ == "__main__":
    unittest.main(verbosity=2)

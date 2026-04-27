# WIREFRAMES — Democracy Without Candidates (ENTERPRISE / PRD LEVEL)

## 📌 DOCUMENT PURPOSE
This document defines enterprise-grade wireframes aligned 100% with the PRD, including:
- Full user flows
- Screen states (loading, error, empty)
- Multi-jurisdiction logic
- Security interactions
- Edge cases

---

# 🌍 1. COUNTRY & JURISDICTION FLOW

## 1.1 Country Selection
[ Brasil ] [ Portugal ] [ Argentina ]

## States:
- Loading countries
- No countries available
- Selection required error

---

# 🔐 2. AUTHENTICATION FLOW

## 2.1 Login
Email
[______]
Password
[______]

[ Login ]

States:
- Invalid credentials
- Loading
- Locked account

---

## 2.2 Registration
Full Name
ID
Birthdate

[ Continue ]

States:
- Invalid ID
- Age restriction
- Duplicate account

---

## 2.3 Biometric Validation

[ Scan Face ]
[ Fingerprint ]

States:
- Success
- Failure (retry)
- Device not supported
- Timeout

---

## 2.4 Reauthentication (Critical Actions)

Trigger:
- Voting
- Signing
- Creating proposal

---

# ⚙️ 3. INITIAL SETUP (PLEBISCITE)

Parameters:
- Quorum
- Signatures %
- Deadlines

States:
- Not started
- Voting open
- Completed
- Failed quorum

---

# 🏠 4. DASHBOARD

Sections:
- Active voting
- Proposals in debate
- Signature collection

States:
- Empty (no proposals)
- Loading
- Error fetching data

---

# 📋 5. PROPOSAL LIST

Filters:
- Type
- Status
- Level

States:
- No results
- Loading
- Filter mismatch

---

# 📄 6. PROPOSAL DETAIL

Components:
- Full text
- Status badge
- Progress bar
- Actions (Sign / Vote)

States:
- Archived
- Closed
- Not eligible user

---

# ✍️ 7. CREATE PROPOSAL

Validation:
- Required fields
- Limit per user/year
- Duplicate content detection

States:
- Draft saved
- Publish success
- Validation error

---

# ✍️ 8. SIGNATURE COLLECTION

Progress bar
Remaining time

States:
- Signature removed
- Limit reached
- Expired

---

# 💬 9. DEBATE MODULE

Features:
- Threaded comments
- Upvote/downvote
- Amendments

States:
- Comment removed
- Abuse reported
- Locked thread

---

# 🗳️ 10. VOTING

Options:
[ YES ] [ NO ] [ ABSTAIN ]

Privacy:
( ) Public
( ) Private

States:
- Already voted
- Vote success
- Vote failed

---

# 📊 11. RESULTS

Display:
- Quorum
- Percentages
- Outcome

States:
- Quorum not reached
- Tie

---

# 🔔 12. NOTIFICATIONS

Events:
- Proposal created
- Voting open
- Deadline reminder

States:
- Read / Unread

---

# 👤 13. PROFILE

Data:
- Stats
- Privacy settings

States:
- Public view
- Private view

---

# 🔐 14. PRIVACY SETTINGS

Options:
- Profile visibility
- Vote default

---

# ⚙️ 15. ADMIN PANEL

Features:
- Country management
- User stats
- System health

States:
- Inactive country
- Setup pending

---

# 📊 16. AUDIT & TRANSPARENCY

Data:
- Logs
- Hash chain
- Verification

States:
- Verified
- Integrity alert

---

# 🔗 17. API VIEW (DEV MODE)

- Endpoint list
- Request/response preview

---

# 🧭 18. GLOBAL STATES (APPLIES TO ALL SCREENS)

- Loading
- Error
- Empty
- Success

---

# 🚀 CONCLUSION

This document represents a production-ready wireframe structure aligned with enterprise-grade systems and the full PRD scope.

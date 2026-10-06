-- Phone numbers of CRM contacts, in E.164. They let phone calls be matched to
-- the contact on the other end, and let people call a contact from the CRM.
-- A contact may have several numbers (mobile, office), listed in the order
-- people entered them; a number may appear on several contacts, e.g. a shared
-- front desk.
CREATE TABLE crm_contact_phone_numbers (
    contact_id UUID NOT NULL REFERENCES crm_contacts(id) ON DELETE CASCADE,
    phone_number TEXT NOT NULL CHECK (phone_number ~ '^\+[1-9][0-9]{6,14}$'),
    position SMALLINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (contact_id, phone_number)
);

-- Look up who is calling.
CREATE INDEX crm_contact_phone_numbers_phone_number ON crm_contact_phone_numbers(phone_number);
